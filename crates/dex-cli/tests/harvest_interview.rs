//! harvest / interview 集成测试（D 组）：staging 转正（confidence 排序 / limit 截断 /
//! 单条失败不阻断）、dry-run 不落盘、未注册源、面试问题集与 stdin 批量提案。

mod common;

use common::{Env, HUMAN_TOKEN, SPOKE_TOKEN};
use dex_core::proposal::{parse_proposal_file, render_proposal_file, ProposalMeta};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command as ProcCommand;

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn last_commit_subject(root: &Path) -> String {
    let out = ProcCommand::new("git")
        .args(["-C", root.to_str().unwrap(), "log", "-1", "--pretty=%s"])
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn staging_names(env: &Env, source: &str) -> Vec<String> {
    let dir = env.path(&format!("inbox/staging/{source}"));
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
                .map(|e| e.file_name().into_string().unwrap())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn inbox_imx_today(env: &Env) -> Vec<String> {
    let prefix = format!("{}-im-x-", today());
    fs::read_dir(env.path("inbox"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".md"))
        .collect()
}

/// 写一条 staging 候选（frontmatter 由 core 渲染，保证可解析）
fn stage_candidate(env: &Env, name: &str, confidence: i64, content: &str) {
    let meta = ProposalMeta {
        source: "im-x".to_string(),
        kind: "fact".to_string(),
        confidence: Some(confidence),
        evidence: format!("im-x:chat#1 摘录「{content}」"),
    };
    let raw = render_proposal_file(&meta, content);
    dex_store::inbox::Inbox::new(&env.root)
        .stage_write("im-x", name, &raw)
        .expect("stage_write");
}

fn parse_json(stdout: &[u8]) -> Value {
    serde_json::from_str(&String::from_utf8_lossy(stdout)).expect("stdout 应为合法 json")
}

fn harvest(env: &Env, extra: &[&str]) -> assert_cmd::Command {
    let mut args: Vec<&str> = vec!["harvest", "--client", "harvest-im-x"];
    args.extend_from_slice(extra);
    env.dex_with_token(&args, Some(SPOKE_TOKEN))
}

/// 三条好候选（confidence 90/85/70）+ 一条坏文件（计入 invalid）
fn stage_standard_set(env: &Env) {
    stage_candidate(env, "c90", 90, "候选九十分的正文");
    stage_candidate(env, "c85", 85, "候选八十五分的正文");
    stage_candidate(env, "c70", 70, "候选七十分的正文");
    fs::write(env.path("inbox/staging/im-x/broken.md"), "不是提案文件").unwrap();
}

// ---------- harvest ----------

#[test]
fn harvest_unregistered_source_exit2() {
    let env = Env::new();
    env.standard_config();
    harvest(&env, &["--from", "nope"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn harvest_promotes_top_confidence_with_limit() {
    let env = Env::new();
    env.standard_config();
    stage_standard_set(&env);

    let out = harvest(&env, &["--from", "im-x", "--limit", "2", "--json"]).unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(v["data"]["source"], "im-x");
    assert_eq!(v["data"]["invalid"], 1, "坏文件跳过并计入 invalid");
    assert_eq!(v["data"]["remaining"], 2, "剩 c70 与坏文件留 staging");

    // 转正两条按 confidence 降序（90/85）；promoted 列表与落盘文件一致
    let promoted = v["data"]["promoted"].as_array().unwrap();
    assert_eq!(promoted.len(), 2);
    let mut confidences: Vec<i64> = promoted
        .iter()
        .map(|p| {
            let file = p["file"].as_str().unwrap();
            let raw = fs::read_to_string(env.path(file)).unwrap();
            parse_proposal_file(&raw).unwrap().0.confidence.unwrap()
        })
        .collect();
    confidences.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(confidences, vec![90, 85], "转正按 confidence 降序取前 N");

    // staging 只剩未被转正的两条；inbox 顶层今日 im-x = 2
    let staging = staging_names(&env, "im-x");
    assert_eq!(staging, vec!["broken.md", "c70.md"]);
    assert_eq!(inbox_imx_today(&env).len(), 2);

    // 批量提交模板（§2.3）
    assert_eq!(last_commit_subject(&env.root), "harvest: stage im-x ×2");
}

#[test]
fn harvest_dry_run_lists_without_side_effects() {
    let env = Env::new();
    env.standard_config();
    stage_standard_set(&env);

    let out = harvest(
        &env,
        &["--from", "im-x", "--limit", "2", "--dry-run", "--json"],
    )
    .unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "dry-run：{v}");
    let promoted = v["data"]["promoted"].as_array().unwrap();
    assert_eq!(promoted.len(), 2, "列出将转正候选");
    let files: Vec<&str> = promoted
        .iter()
        .map(|p| p["file"].as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        vec!["c90.md", "c85.md"],
        "dry-run 列表 = staging 文件名"
    );
    assert_eq!(v["data"]["remaining"], 2);

    // 不落盘：staging 原样、inbox 无新增、无新提交
    assert_eq!(staging_names(&env, "im-x").len(), 4);
    assert!(inbox_imx_today(&env).is_empty());
    assert_eq!(last_commit_subject(&env.root), "init: test fixture");
}

#[test]
fn harvest_single_rejection_does_not_block_batch() {
    let env = Env::new();
    env.standard_config();
    // 两条好候选 + 一条密钥候选（单条拒绝留 staging，不阻断批次）
    stage_candidate(&env, "good1", 90, "正常候选");
    stage_candidate(&env, "good2", 85, "正常候选二");
    let secret_meta = ProposalMeta {
        source: "im-x".to_string(),
        kind: "fact".to_string(),
        confidence: Some(99),
        evidence: "im-x:chat#9 摘录".to_string(),
    };
    let raw = render_proposal_file(&secret_meta, "泄漏 AKIAIOSFODNN7EXAMPLE");
    dex_store::inbox::Inbox::new(&env.root)
        .stage_write("im-x", "secret", &raw)
        .unwrap();

    let out = harvest(&env, &["--from", "im-x", "--json"]).unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "单条拒绝不阻断批次：{v}");
    let promoted = v["data"]["promoted"].as_array().unwrap();
    assert_eq!(promoted.len(), 3);
    let errored: Vec<&Value> = promoted.iter().filter(|p| !p["error"].is_null()).collect();
    assert_eq!(errored.len(), 1, "密钥候选记录 error：{promoted:?}");
    assert!(errored[0]["error"].as_str().unwrap().contains("密钥"));
    // secret 留 staging、两条转正
    assert_eq!(staging_names(&env, "im-x"), vec!["secret.md"]);
    assert_eq!(inbox_imx_today(&env).len(), 2);
    assert_eq!(last_commit_subject(&env.root), "harvest: stage im-x ×2");
}

// ---------- interview ----------

#[test]
fn interview_core_question_set_json() {
    let env = Env::new();
    env.standard_config();
    let out = env
        .admin(&["interview", "--client", "human", "--json"])
        .unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["round"], "core");
    let questions = v["data"]["questions"].as_array().unwrap();
    assert_eq!(questions.len(), 5, "首轮 5 核心问：{questions:?}");
    assert!(questions[0].as_str().unwrap().contains("角色与主业"));
    assert!(!v["data"]["guidance"].as_str().unwrap().is_empty());
}

#[test]
fn interview_follow_up_guidance() {
    let env = Env::new();
    env.standard_config();
    let out = env
        .admin(&[
            "interview",
            "--client",
            "human",
            "--round",
            "follow-up",
            "--json",
        ])
        .unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["data"]["round"], "follow-up");
    assert!(
        v["data"]["guidance"].as_str().unwrap().contains("补问"),
        "渐进补全引导：{v}"
    );
}

#[test]
fn interview_unknown_round_exit2() {
    let env = Env::new();
    env.standard_config();
    env.admin(&["interview", "--client", "human", "--round", "bogus"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn interview_stdin_batch_proposes_via_guard() {
    let env = Env::new();
    env.standard_config();
    let out = env
        .admin(&["interview", "--client", "human", "--stdin", "--json"])
        .write_stdin("1: 答案一\n2: 答案二\nQ3: 答案三\n")
        .unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "stdin 批量：{v}");
    assert_eq!(v["data"]["items"].as_array().unwrap().len(), 3);

    // 三条答案 → inbox 三文件（source=human、kind=fact、evidence 指回面试原文）
    let date = today();
    let prefix = format!("{date}-human-");
    let mut files: Vec<String> = fs::read_dir(env.path("inbox"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".md"))
        .collect();
    assert_eq!(files.len(), 3, "三条答案三文件：{files:?}");
    files.sort();
    let mut evidence_set: Vec<String> = Vec::new();
    for name in &files {
        let raw = fs::read_to_string(env.path(&format!("inbox/{name}"))).unwrap();
        let (meta, content) = parse_proposal_file(&raw).unwrap();
        assert_eq!(meta.source, "human");
        assert_eq!(meta.kind, "fact");
        assert!(content.starts_with("答案"), "content={content}");
        evidence_set.push(meta.evidence);
    }
    evidence_set.sort();
    assert_eq!(
        evidence_set,
        vec![
            format!("interview core {date} Q1"),
            format!("interview core {date} Q2"),
            format!("interview core {date} Q3"),
        ],
        "evidence = interview <round> <date> Q<n>"
    );

    // 每条独立提交留痕（§2.3 模板，source = human）
    assert_eq!(last_commit_subject(&env.root), "inbox: propose from human");
}

/// 审计面：harvest 密钥拒绝写 audit 且不含密钥原文；唯一候选失败 → 全部失败透传首条错误（exit 9）
#[test]
fn harvest_secret_rejection_audited_clean() {
    let env = Env::new();
    env.standard_config();
    let secret_meta = ProposalMeta {
        source: "im-x".to_string(),
        kind: "fact".to_string(),
        confidence: Some(99),
        evidence: "im-x:chat#9 摘录".to_string(),
    };
    let raw = render_proposal_file(&secret_meta, "泄漏 AKIAIOSFODNN7EXAMPLE");
    dex_store::inbox::Inbox::new(&env.root)
        .stage_write("im-x", "secret", &raw)
        .unwrap();

    // 唯一候选被拒 → 全部失败 → 透传首条错误（E_SECRET · 9）
    harvest(&env, &["--from", "im-x"])
        .assert()
        .failure()
        .code(9);
    assert_eq!(
        staging_names(&env, "im-x"),
        vec!["secret.md"],
        "被拒候选留 staging"
    );
    let audit = fs::read_to_string(env.path(".cache/audit.log")).unwrap_or_default();
    assert!(audit.contains("E_SECRET"), "拒绝事件入 audit：{audit}");
    assert!(!audit.contains("AKIA"), "密钥原文不入审计面");
    assert!(!audit.contains(SPOKE_TOKEN) && !audit.contains(HUMAN_TOKEN));
}
