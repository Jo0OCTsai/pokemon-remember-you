//! propose / journal 集成测试（D 组）：守卫失败路径、幂等重放、限流、advisory、
//! git 缺失降级、journal 小节追加语义。

mod common;

use common::{Env, SPOKE_TOKEN};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command as ProcCommand;

/// 今日本地日期（与命令内 today_local 同源：chrono Local）
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 仓库最后一次 commit 主题行
fn last_commit_subject(root: &Path) -> String {
    let out = ProcCommand::new("git")
        .args(["-C", root.to_str().unwrap(), "log", "-1", "--pretty=%s"])
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// inbox 顶层 .md 文件名列表
fn inbox_top_level(env: &Env) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(env.path("inbox"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".md"))
        .collect();
    names.sort();
    names
}

fn audit_log(env: &Env) -> String {
    fs::read_to_string(env.path(".cache/audit.log")).unwrap_or_default()
}

fn propose(env: &Env, extra: &[&str], msg: &str) -> assert_cmd::Command {
    let mut args: Vec<&str> = vec!["propose", "--client", "choose-you"];
    args.extend_from_slice(extra);
    args.push(msg);
    env.dex_with_token(&args, Some(SPOKE_TOKEN))
}

fn parse_json(stdout: &[u8]) -> Value {
    serde_json::from_str(&String::from_utf8_lossy(stdout)).expect("stdout 应为合法 json")
}

// ---------- propose：成功与落盘 ----------

#[test]
fn propose_ok_writes_file_and_commits() {
    let env = Env::new();
    env.standard_config();
    let out = propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--confidence",
            "85",
            "--evidence",
            "chat #1",
            "--json",
        ],
        "测试提案正文",
    )
    .unwrap();

    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "信封：{v}");
    let file = v["data"]["file"].as_str().unwrap().to_string();
    let date = today();
    assert!(
        file.starts_with(&format!("inbox/{date}-choose-you-")) && file.ends_with(".md"),
        "标准命名式：{file}"
    );
    assert_eq!(v["data"]["idempotent"], false);
    assert_eq!(v["data"]["committed"], true);

    // 落盘文件与 frontmatter 往返（§2.3 示例形）
    let raw = fs::read_to_string(env.path(&file)).unwrap();
    assert_eq!(
        raw,
        "---\nsource: choose-you\nkind: fact\nconfidence: 85\nevidence: chat #1\n---\n测试提案正文"
    );

    // git 留痕（FR-4.5 模板）
    assert_eq!(
        last_commit_subject(&env.root),
        "inbox: propose from choose-you"
    );
}

#[test]
fn propose_stdin_dash_reads_body() {
    let env = Env::new();
    env.standard_config();
    let out = propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--evidence",
            "chat #2",
            "--json",
        ],
        "-",
    )
    .write_stdin("来自 stdin 的正文\n")
    .unwrap();
    let v = parse_json(&out.stdout);
    let file = v["data"]["file"].as_str().unwrap();
    let raw = fs::read_to_string(env.path(file)).unwrap();
    assert_eq!(
        raw,
        "---\nsource: choose-you\nkind: fact\nevidence: chat #2\n---\n来自 stdin 的正文"
    );
}

#[test]
fn propose_idempotent_replay_returns_existing_file() {
    let env = Env::new();
    env.standard_config();
    let args = [
        "--source",
        "choose-you",
        "--kind",
        "fact",
        "--evidence",
        "chat #3",
        "--json",
    ];
    let first = propose(&env, &args, "幂等测试内容").unwrap();
    let v1 = parse_json(&first.stdout);
    let file1 = v1["data"]["file"].as_str().unwrap().to_string();
    assert_eq!(v1["data"]["idempotent"], false);

    // 重放：confidence/evidence 不同不破坏幂等（§5.5-7）
    let second = propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--confidence",
            "10",
            "--evidence",
            "另一份证据",
            "--json",
        ],
        "幂等测试内容",
    )
    .unwrap();
    let v2 = parse_json(&second.stdout);
    assert_eq!(v2["data"]["file"].as_str().unwrap(), file1);
    assert_eq!(v2["data"]["idempotent"], true);
    assert_eq!(v2["data"]["committed"], false);
    assert_eq!(inbox_top_level(&env).len(), 1, "不重复落盘");
}

// ---------- propose：守卫失败路径（每退出码至少一例） ----------

#[test]
fn propose_no_evidence_exit4() {
    let env = Env::new();
    env.standard_config();
    propose(
        &env,
        &["--source", "choose-you", "--kind", "fact", "--evidence", ""],
        "正文",
    )
    .assert()
    .failure()
    .code(4);
}

#[test]
fn propose_content_too_large_exit5() {
    let env = Env::new();
    env.standard_config();
    let big = "字".repeat(4001);
    propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--evidence",
            "chat #4",
        ],
        &big,
    )
    .assert()
    .failure()
    .code(5);
    assert!(inbox_top_level(&env).is_empty(), "拒绝不落盘");
}

#[test]
fn propose_source_mismatch_exit2_and_audit() {
    let env = Env::new();
    env.standard_config();
    // choose-you 客户端 allowed_sources = ["choose-you"]，source=human → 绑定失败
    propose(
        &env,
        &[
            "--source",
            "human",
            "--kind",
            "fact",
            "--evidence",
            "chat #5",
        ],
        "正文",
    )
    .assert()
    .failure()
    .code(2);

    // 审计：拒绝行存在且无内容/token（FR-10.6）
    let audit = audit_log(&env);
    assert!(
        audit.contains("E_SOURCE_MISMATCH") && audit.contains("choose-you"),
        "audit 应含 E_SOURCE_MISMATCH：{audit}"
    );
    assert!(!audit.contains(SPOKE_TOKEN), "token 不入审计面");
    assert!(!audit.contains("正文"), "记忆内容不入审计面");
}

#[test]
fn propose_rate_limit_exit5() {
    let env = Env::new();
    env.standard_config();
    // 造满当日配额：proposals_per_day = 20（standard_config）
    let date = today();
    for i in 0..20 {
        let name = format!("{date}-choose-you-c{i:05}.md");
        fs::write(env.path(&format!("inbox/{name}")), "").unwrap();
    }
    propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--evidence",
            "chat #6",
        ],
        "第 21 条",
    )
    .assert()
    .failure()
    .code(5);
    assert!(audit_log(&env).contains("E_RATE_LIMIT"));
    assert_eq!(inbox_top_level(&env).len(), 20, "拒绝不落盘");
}

#[test]
fn propose_secret_exit9_and_audit_clean() {
    let env = Env::new();
    env.standard_config();
    propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--evidence",
            "chat #7",
        ],
        "泄漏 AKIAIOSFODNN7EXAMPLE",
    )
    .assert()
    .failure()
    .code(9);
    let audit = audit_log(&env);
    assert!(audit.contains("E_SECRET"), "audit 应含 E_SECRET：{audit}");
    assert!(!audit.contains("AKIA"), "密钥原文不入审计面");
    assert!(inbox_top_level(&env).is_empty(), "密钥拒绝不落盘");
}

#[test]
fn propose_secret_advisory_exit0_with_warning() {
    let env = Env::new();
    // 本机 config：[guard] secret_advisory = true（FR-4.6）
    env.machine_config(&format!(
        r#"root = "{root}"

[guard]
secret_advisory = true

[clients."choose-you"]
scopes = ["person", "apps/todo"]
propose = true
rate_limit = {{ proposals_per_day = 20, journal_per_day = 60 }}
allowed_sources = ["choose-you"]
"#,
        root = env.root.display()
    ));
    env.credentials(&[("choose-you", SPOKE_TOKEN)]);

    let out = propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--evidence",
            "chat #8",
            "--json",
        ],
        "泄漏 AKIAIOSFODNN7EXAMPLE",
    )
    .unwrap();
    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "advisory 放行：{v}");
    let codes: Vec<&str> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"W_SECRET"), "warnings：{codes:?}");
    assert_eq!(v["data"]["committed"], true);
}

#[test]
fn propose_git_missing_degrades_to_warning() {
    let env = Env::new();
    env.standard_config();
    let out = propose(
        &env,
        &[
            "--source",
            "choose-you",
            "--kind",
            "fact",
            "--evidence",
            "chat #9",
            "--json",
        ],
        "无 git 环境的提案",
    )
    .env("PATH", "")
    .unwrap();

    let v = parse_json(&out.stdout);
    assert_eq!(v["ok"], true, "git 缺失降级成功：{v}");
    assert_eq!(v["data"]["committed"], false);
    let codes: Vec<&str> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"W_GIT_UNAVAILABLE"), "warnings：{codes:?}");
    // 数据不丢：文件已落盘
    assert_eq!(inbox_top_level(&env).len(), 1);
}

// ---------- journal ----------

fn journal(env: &Env, extra: &[&str], msg: &str) -> assert_cmd::Command {
    let mut args: Vec<&str> = vec!["journal", "--client", "choose-you"];
    args.extend_from_slice(extra);
    args.push(msg);
    env.dex_with_token(&args, Some(SPOKE_TOKEN))
}

#[test]
fn journal_creates_page_with_section() {
    let env = Env::new();
    env.standard_config();
    let out = journal(
        &env,
        &["--source", "choose-you", "--json"],
        "捕捉 3 / 逃走 2",
    )
    .unwrap();
    let v = parse_json(&out.stdout);
    let date = today();
    assert_eq!(v["data"]["page"], format!("journal/{date}.md"));
    assert_eq!(v["data"]["section"], "供稿 · choose-you");
    assert_eq!(v["data"]["committed"], true);

    let page = fs::read_to_string(env.path(&format!("journal/{date}.md"))).unwrap();
    assert_eq!(
        page,
        format!("# {date}\n\n## 供稿 · choose-you\n- 捕捉 3 / 逃走 2\n")
    );
    assert_eq!(
        last_commit_subject(&env.root),
        "journal: append from choose-you"
    );
}

#[test]
fn journal_appends_new_section_keeps_handwritten() {
    let env = Env::new();
    env.standard_config();
    let date = today();
    let page = format!("# {date}\n## 手写\n- 人写的内容\n");
    fs::write(env.path(&format!("journal/{date}.md")), page).unwrap();

    journal(&env, &["--source", "choose-you"], "首条供稿").unwrap();

    let page = fs::read_to_string(env.path(&format!("journal/{date}.md"))).unwrap();
    assert_eq!(
        page,
        format!("# {date}\n## 手写\n- 人写的内容\n\n## 供稿 · choose-you\n- 首条供稿\n"),
        "手写小节不动，供稿小节建于文件末"
    );
}

#[test]
fn journal_repeat_same_day_appends_tail_and_multi_source() {
    let env = Env::new();
    env.standard_config();
    journal(&env, &["--source", "choose-you"], "一").unwrap();
    journal(&env, &["--source", "choose-you"], "二").unwrap();
    // 第二来源：human 客户端（allowed_sources = ["human"]）
    env.dex_with_token(
        &[
            "journal",
            "--client",
            "human",
            "--source",
            "human",
            "人的供稿",
        ],
        Some(common::HUMAN_TOKEN),
    )
    .unwrap();

    let date = today();
    let page = fs::read_to_string(env.path(&format!("journal/{date}.md"))).unwrap();
    // 同日重复供稿追加至小节尾部；多来源小节隔离（FR-5.2）
    assert_eq!(
        page,
        format!("# {date}\n\n## 供稿 · choose-you\n- 一\n- 二\n\n## 供稿 · human\n- 人的供稿\n")
    );
}

#[test]
fn journal_stdin_dash() {
    let env = Env::new();
    env.standard_config();
    journal(&env, &["--source", "choose-you"], "-")
        .write_stdin("来自 stdin 的供稿\n")
        .unwrap();
    let date = today();
    let page = fs::read_to_string(env.path(&format!("journal/{date}.md"))).unwrap();
    assert!(page.contains("- 来自 stdin 的供稿"), "{page}");
}

#[test]
fn journal_rate_limit_exit5() {
    let env = Env::new();
    env.standard_config();
    let date = today();
    // journal_per_day 缺省 60：造满 60 条已供稿
    let mut page = format!("# {date}\n\n## 供稿 · choose-you\n");
    for i in 0..60 {
        page.push_str(&format!("- 已供稿 {i}\n"));
    }
    fs::write(env.path(&format!("journal/{date}.md")), page).unwrap();

    journal(&env, &["--source", "choose-you"], "第 61 条")
        .assert()
        .failure()
        .code(5);
    assert!(audit_log(&env).contains("E_RATE_LIMIT"));
}

#[test]
fn journal_bad_date_exit2() {
    let env = Env::new();
    env.standard_config();
    journal(
        &env,
        &["--source", "choose-you", "--date", "2026-02-30"],
        "正文",
    )
    .assert()
    .failure()
    .code(2);
    // 非规范形态（非零填充）同样拒绝
    journal(
        &env,
        &["--source", "choose-you", "--date", "2026-9-5"],
        "正文",
    )
    .assert()
    .failure()
    .code(2);
}
