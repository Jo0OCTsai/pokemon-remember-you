//! D5 集成测试：`dex stale` / `dex review` / `dex lint`（FR-6.5/6.6/6.11）。
//!
//! 口径：管理命令一律 `env.admin()`（--client human + DEX_TOKEN，非 TTY 需凭证）；
//! git 缺失场景以 PATH 指向空目录模拟（mtime 回退 + W_GIT_UNAVAILABLE）。

mod common;

use common::Env;
use std::path::{Path, PathBuf};

fn parse_json(stdout: &[u8]) -> serde_json::Value {
    serde_json::from_str(String::from_utf8_lossy(stdout).trim()).expect("stdout 应为合法 json")
}

fn out_string(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout).to_string()
}

fn finding_checks(v: &serde_json::Value) -> Vec<String> {
    v["data"]["findings"]
        .as_array()
        .expect("data.findings 应为数组")
        .iter()
        .map(|f| f["check"].as_str().expect("check 应为字符串").to_string())
        .collect()
}

/// 今天前 `days` 天的日期（YYYY-MM-DD，本地时区）
fn date_days_ago(days: i64) -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

/// 把文件 mtime 设为约 `days` 天前（FileTimes，Rust 1.75+）
fn set_mtime_days_ago(path: &Path, days: i64) {
    use std::fs::FileTimes;
    let f = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("打开文件");
    let past = std::time::SystemTime::now()
        - std::time::Duration::from_secs((days.unsigned_abs() + 1) * 86400);
    f.set_times(FileTimes::new().set_modified(past))
        .expect("设置 mtime");
}

/// 不含 git 的 PATH（空目录）——模拟 git 缺失
fn no_git_path(env: &Env) -> PathBuf {
    let dir = env.home.join("empty-path");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn valid_proposal(source: &str, content: &str) -> String {
    format!(
        "---\nsource: {source}\nkind: fact\nconfidence: 80\nevidence: chat #1\n---\n{content}\n"
    )
}

// =============================== lint ===============================

#[test]
fn lint_clean_repo_exit0_no_findings() {
    let env = Env::new();
    env.standard_config();
    let out = env.admin(&["lint", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    assert!(v["ok"].as_bool().unwrap(), "干净仓库信封 ok=true：{v}");
    assert_eq!(
        v["data"]["findings"].as_array().map(Vec::len),
        Some(0),
        "干净仓库零 findings：{v}"
    );
}

#[test]
fn lint_top_level_outside_whitelist_error_exit1() {
    let env = Env::new();
    env.standard_config();
    std::fs::create_dir_all(env.path("misc")).unwrap();
    std::fs::write(env.path("misc/x.md"), "# M\n- m\n").unwrap();

    let out = env.admin(&["lint", "--json"]).assert().code(1);
    let v = parse_json(&out.get_output().stdout);
    // 信封特例（§8.4）：error 级存在 → 退出码 1，但 ok=true、问题走 data.findings
    assert!(v["ok"].as_bool().unwrap(), "lint 信封特例 ok=true：{v}");
    let checks = finding_checks(&v);
    assert!(
        checks.contains(&"top-level-whitelist".to_string()),
        "{checks:?}"
    );

    // text 行格式：[error] check path message
    let t = env.admin(&["lint"]).assert().code(1);
    let s = out_string(&t.get_output().stdout);
    assert!(s.contains("[error] top-level-whitelist misc:"), "{s}");
}

#[test]
fn lint_frontmatter_residue_error() {
    let env = Env::new();
    env.standard_config();
    std::fs::write(
        env.path("person/f.md"),
        "---\nsource: x\nkind: fact\nevidence: e\n---\n# T\n- a\n",
    )
    .unwrap();
    let out = env.admin(&["lint", "--json"]).assert().code(1);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(
        checks.contains(&"frontmatter-residue".to_string()),
        "{checks:?}"
    );
}

#[test]
fn lint_bad_comment_format_error() {
    let env = Env::new();
    env.standard_config();
    // 游离注释：空行后（不挂靠 H1 也不挂靠条目行）
    std::fs::write(
        env.path("person/c.md"),
        "# 注释\n- 条目\n\n<!-- src: journal 2026-01-01 -->\n",
    )
    .unwrap();
    // keep-until 日期形状非法（条目级位置合法，值非法）
    std::fs::write(
        env.path("person/k.md"),
        "# K\n- a\n<!-- keep-until: 2026-1-1 x -->\n",
    )
    .unwrap();
    let out = env.admin(&["lint", "--json"]).assert().code(1);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    let comment_findings = checks.iter().filter(|c| *c == "comment-format").count();
    assert_eq!(comment_findings, 2, "{checks:?}");
}

#[test]
fn lint_comment_format_valid_positions_clean() {
    let env = Env::new();
    env.standard_config();
    // 文件级（紧随 H1）+ 条目级（紧随条目行）+ 行尾：全部合法
    std::fs::write(
        env.path("person/ok.md"),
        "# 备注\n<!-- src: choose-you 固化 2026-09 -->\n- a <!-- src: x -->\n- b\n<!-- keep-until: 2099-12-31 理由 -->\n",
    )
    .unwrap();
    let out = env.admin(&["lint", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(
        !checks.contains(&"comment-format".to_string()),
        "{checks:?}"
    );
}

#[test]
fn lint_inbox_stale_over_7_days_error() {
    let env = Env::new();
    env.standard_config();
    let name = format!("{}-choose-you-ab12cd.md", date_days_ago(8));
    std::fs::write(
        env.path(&format!("inbox/{name}")),
        valid_proposal("choose-you", "周报习惯转为周四下午整理"),
    )
    .unwrap();
    let out = env.admin(&["lint", "--json"]).assert().code(1);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(checks.contains(&"inbox-stale".to_string()), "{checks:?}");
    // 命名/字段合法 → 不应有 inbox-naming
    assert!(!checks.contains(&"inbox-naming".to_string()), "{checks:?}");
}

#[test]
fn lint_inbox_staging_exempt() {
    let env = Env::new();
    env.standard_config();
    std::fs::create_dir_all(env.path("inbox/staging/im-x")).unwrap();
    let p = env.path("inbox/staging/im-x/old.md");
    std::fs::write(&p, valid_proposal("im-x", "暂存候选")).unwrap();
    set_mtime_days_ago(&p, 10);
    let out = env.admin(&["lint", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(
        !checks.contains(&"inbox-stale".to_string()),
        "staging 豁免滞留检查：{checks:?}"
    );
    assert_eq!(checks.len(), 0, "整体应干净：{checks:?}");
}

#[test]
fn lint_bootstrap_migration_warning_exit0() {
    let env = Env::new();
    env.standard_config();
    std::fs::create_dir_all(env.path("inbox/bootstrap")).unwrap();
    std::fs::write(env.path("inbox/bootstrap/draft.md"), "# d\n- x\n").unwrap();
    // 仅 warning 级 → 退出码 0
    let out = env.admin(&["lint", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    assert!(v["ok"].as_bool().unwrap());
    let checks = finding_checks(&v);
    assert!(checks.contains(&"inbox-stale".to_string()), "{checks:?}");
    // warning 级同时进 warnings[]（信封特例）
    let w = v["warnings"].as_array().unwrap();
    assert!(
        w.iter()
            .any(|x| x["message"].as_str().unwrap_or("").contains("bootstrap")),
        "warnings[] 应含 bootstrap 迁移提示：{w:?}"
    );
}

#[test]
fn lint_domains_budget_over_8_warning_exit0() {
    let env = Env::new();
    env.standard_config();
    for d in 1..=9 {
        let dir = format!("domains/d{d}");
        std::fs::create_dir_all(env.path(&dir)).unwrap();
        std::fs::write(env.path(&format!("{dir}/x.md")), "# X\n- x\n").unwrap();
    }
    let out = env.admin(&["lint", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(checks.contains(&"domains-budget".to_string()), "{checks:?}");
}

#[test]
fn lint_secret_pattern_error() {
    let env = Env::new();
    env.standard_config();
    std::fs::write(
        env.path("person/secret.md"),
        "# 密钥\n- 泄漏的凭据 AKIAIOSFODNN7EXAMPLE 请处理\n",
    )
    .unwrap();
    let out = env.admin(&["lint", "--json"]).assert().code(1);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(checks.contains(&"secret-pattern".to_string()), "{checks:?}");
}

#[test]
fn lint_scope_filter() {
    let env = Env::new();
    env.standard_config();
    std::fs::create_dir_all(env.path("misc")).unwrap();
    std::fs::write(
        env.path("person/f.md"),
        "---\nsource: x\nkind: fact\nevidence: e\n---\n# T\n- a\n",
    )
    .unwrap();
    let out = env
        .admin(&["lint", "--scope", "person", "--json"])
        .assert()
        .code(1);
    let v = parse_json(&out.get_output().stdout);
    let checks = finding_checks(&v);
    assert!(
        checks.contains(&"frontmatter-residue".to_string()),
        "{checks:?}"
    );
    assert!(
        !checks.contains(&"top-level-whitelist".to_string()),
        "--scope person 应滤掉 misc/ 的顶层发现：{checks:?}"
    );
}

// =============================== stale ===============================

#[test]
fn stale_fresh_repo_empty_exit0() {
    let env = Env::new();
    env.standard_config();
    let out = env.admin(&["stale", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    assert!(v["ok"].as_bool().unwrap());
    assert_eq!(v["data"]["count"].as_i64(), Some(0), "新鲜仓库空清单：{v}");
    assert_eq!(v["data"]["candidates"].as_array().map(Vec::len), Some(0));
}

#[test]
fn stale_mtime_fallback_git_unavailable_and_keep_until() {
    let env = Env::new();
    env.standard_config();
    // 91 天前 mtime：git 缺失 → 全体 mtime 回退 + W_GIT_UNAVAILABLE
    set_mtime_days_ago(&env.path("person/profile.md"), 91);
    // keep-until 未到期（2099）→ 即使文件陈旧也不列
    set_mtime_days_ago(&env.path("person/preferences.md"), 91);
    // keep-until 已到期 → 无论年龄强制复审
    let expired = format!(
        "# 复审\n- 旧结论\n<!-- keep-until: {} 项目暂停 -->\n",
        date_days_ago(30)
    );
    std::fs::write(env.path("person/expired.md"), expired).unwrap();

    let mut cmd = env.admin(&["stale", "--json"]);
    cmd.env("PATH", no_git_path(&env));
    let out = cmd.assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    assert!(v["ok"].as_bool().unwrap(), "降级成功：{v}");
    let codes: Vec<String> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_string())
        .collect();
    assert!(
        codes.contains(&"W_GIT_UNAVAILABLE".to_string()),
        "应带 W_GIT_UNAVAILABLE：{codes:?}"
    );
    let cands = v["data"]["candidates"].as_array().unwrap();
    let paths: Vec<&str> = cands.iter().map(|c| c["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"person/profile.md"), "{paths:?}");
    assert!(
        !paths.contains(&"person/preferences.md"),
        "keep-until 未到期豁免：{paths:?}"
    );
    assert!(paths.contains(&"person/expired.md"), "{paths:?}");
    let exp = cands
        .iter()
        .find(|c| c["path"] == "person/expired.md")
        .unwrap();
    assert_eq!(exp["force_review"], serde_json::json!(true));

    // text：!复审 标记
    let mut cmd2 = env.admin(&["stale"]);
    cmd2.env("PATH", no_git_path(&env));
    let t = cmd2.assert().code(0);
    let s = out_string(&t.get_output().stdout);
    assert!(s.contains("person/profile.md:3"), "{s}");
    assert!(s.contains("!复审"), "{s}");
    assert!(s.contains("| 建议 keep"), "{s}");
}

#[test]
fn stale_days_override_only_default_window() {
    let env = Env::new();
    // per-scope 分档：person = 5 天（不受 --days 覆盖）
    env.machine_config(&format!(
        "root = \"{}\"\n\n[stale]\ndays = 90\n\n[stale.scopes.person]\ndays = 5\n",
        env.root.display()
    ));
    set_mtime_days_ago(&env.path("person/profile.md"), 10);

    // --days 90 覆盖的是缺省窗口；person 分档 5 天仍生效 → 仍列入
    let mut cmd = env.admin(&["stale", "--days", "90", "--json"]);
    cmd.env("PATH", no_git_path(&env));
    let out = cmd.assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    let paths: Vec<&str> = v["data"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert!(
        paths.contains(&"person/profile.md"),
        "per-scope 5 天档不受 --days 90 覆盖：{paths:?}"
    );

    // 无 --days 时 domains 走缺省 90：10 天前的 domains 文件不列
    set_mtime_days_ago(&env.path("domains/coding/deploy.md"), 10);
    let mut cmd2 = env.admin(&["stale", "--json"]);
    cmd2.env("PATH", no_git_path(&env));
    let out2 = cmd2.assert().code(0);
    let v2 = parse_json(&out2.get_output().stdout);
    let paths2: Vec<&str> = v2["data"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert!(
        !paths2.contains(&"domains/coding/deploy.md"),
        "缺省 90 天：10 天前的 domains 文件不列：{paths2:?}"
    );

    // --days 5 覆盖缺省窗口 → domains 文件进入
    let mut cmd3 = env.admin(&["stale", "--days", "5", "--json"]);
    cmd3.env("PATH", no_git_path(&env));
    let out3 = cmd3.assert().code(0);
    let v3 = parse_json(&out3.get_output().stdout);
    let paths3: Vec<&str> = v3["data"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert!(
        paths3.contains(&"domains/coding/deploy.md"),
        "--days 5 覆盖缺省窗口：{paths3:?}"
    );
}

// =============================== review ===============================

/// review 测试共用仓库：inbox 待裁决 + 今日 journal 页 + 跨 apps 同 topic + person 近义对
fn review_env() -> Env {
    let env = Env::new();
    env.standard_config();
    let today = today();

    // ②：inbox 待裁决提案（今日、合法命名与字段）
    std::fs::write(
        env.path(&format!("inbox/{today}-choose-you-ab12cd.md")),
        valid_proposal("choose-you", "周报习惯转为周四下午整理"),
    )
    .unwrap();

    // ③：今日 journal 页（列表项 = 提升候选素材）
    std::fs::write(
        env.path(&format!("journal/{today}.md")),
        format!("# {today}\n- 捕获一条待提升的模式\n"),
    )
    .unwrap();

    // ⑤：同 topic 跨 2 个 apps scope → 升级候选（同时构成 ⑥ 矛盾组）
    std::fs::write(env.path("apps/todo/shared.md"), "# 共享主题\n- A 侧条目\n").unwrap();
    std::fs::create_dir_all(env.path("apps/second")).unwrap();
    std::fs::write(
        env.path("apps/second/shared.md"),
        "# 共享主题\n- B 侧条目\n",
    )
    .unwrap();

    // ⑥：同 scope（person）内内容相同的近义对（topic 不同 → 非矛盾组）
    std::fs::write(
        env.path("person/syn-a.md"),
        "# 主题甲\n- 深夜工作容易误判\n",
    )
    .unwrap();
    std::fs::write(
        env.path("person/syn-b.md"),
        "# 主题乙\n- 深夜工作容易误判\n",
    )
    .unwrap();

    env
}

#[test]
fn review_section_order_text_and_json_keys() {
    let env = review_env();

    // text：段序 = FR-6.6 权威序（逐段断言标题顺序）
    let out = env.admin(&["review"]).assert().code(0);
    let s = out_string(&out.get_output().stdout);
    let headers = [
        "## ① lint",
        "## ② inbox",
        "## ③ journal",
        "## ④ 衰减清单",
        "## ⑤ scope 升降级",
        "## ⑥ 近义与矛盾",
        "## ⑦ 结构整理",
    ];
    let mut last = 0;
    for h in headers {
        let pos = s.find(h).unwrap_or_else(|| panic!("缺段标题 {h}：\n{s}"));
        assert!(pos >= last, "段序错乱：{h} 出现在上一段之前：\n{s}");
        last = pos;
    }
    // ② 内容与建议命令（FR-9.2）
    assert!(s.contains("inbox 待裁决提案（1）"), "{s}");
    assert!(s.contains("否决：rm inbox/"), "{s}");
    // ③ 建议命令
    assert!(s.contains("dex read journal/"), "{s}");
    // ④ 周报粘贴区占位
    assert!(s.contains("Spoke 使用周报粘贴区"), "{s}");

    // json：sections 键齐 + inbox_pending + groups
    let out = env.admin(&["review", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    assert!(v["ok"].as_bool().unwrap());
    assert_eq!(v["data"]["inbox_pending"].as_i64(), Some(1));
    let sections = v["data"]["sections"]
        .as_object()
        .expect("sections 应为对象");
    let mut keys: Vec<&str> = sections.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "inbox",
            "journal",
            "lint",
            "near_synonyms",
            "scope_moves",
            "stale",
            "structure"
        ],
        "七段 sections 键齐：{keys:?}"
    );
    // ② inbox 提案字段
    let inbox = sections["inbox"].as_array().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0]["source"], "choose-you");
    assert_eq!(inbox[0]["kind"], "fact");
    assert_eq!(inbox[0]["confidence"], serde_json::json!(80));
    assert_eq!(inbox[0]["evidence"], "chat #1");
    // ③ journal 列表项
    let journal = sections["journal"].as_array().unwrap();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["items"].as_array().unwrap().len(), 1);
    // ⑤ 升级候选
    let moves = sections["scope_moves"].as_array().unwrap();
    assert!(
        moves
            .iter()
            .any(|m| m["topic"] == "共享主题" && m["scopes"].as_array().unwrap().len() == 2),
        "{moves:?}"
    );
    // ⑥ 矛盾组 + 近义组
    let near = &sections["near_synonyms"];
    let contras = near["contradictions"].as_array().unwrap();
    assert!(
        contras.iter().any(|c| c["topic"] == "共享主题"),
        "{contras:?}"
    );
    let syns = near["synonyms"].as_array().unwrap();
    assert_eq!(syns.len(), 1, "person 近义对应成组：{syns:?}");
    assert_eq!(syns[0]["scope"], "person");
    assert_eq!(syns[0]["members"].as_array().unwrap().len(), 2);
    // groups 含 apps / person 组
    let groups = v["data"]["groups"].as_array().unwrap();
    let group_names: Vec<&str> = groups
        .iter()
        .map(|g| g["group"].as_str().unwrap())
        .collect();
    assert!(group_names.contains(&"apps"), "{group_names:?}");
    assert!(group_names.contains(&"person"), "{group_names:?}");
}

#[test]
fn review_inbox_pending_warning() {
    let env = review_env();
    // json：warnings[] 带 FR-4.4 提示
    let out = env.admin(&["review", "--json"]).assert().code(0);
    let v = parse_json(&out.get_output().stdout);
    let msgs: Vec<String> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["message"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("inbox 未清空")),
        "warnings 应含 inbox 未清空：{msgs:?}"
    );
    // text：尾部提示
    let out = env.admin(&["review"]).assert().code(0);
    let s = out_string(&out.get_output().stdout);
    assert!(s.contains("inbox 未清空（FR-4.4 周清空约束）"), "{s}");
}

#[test]
fn review_group_filter() {
    let env = review_env();
    let out = env
        .admin(&["review", "--group", "person", "--json"])
        .assert()
        .code(0);
    let v = parse_json(&out.get_output().stdout);
    let groups: Vec<&str> = v["data"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["group"].as_str().unwrap())
        .collect();
    assert_eq!(groups, vec!["person"], "仅输出该组：{groups:?}");
    let sections = &v["data"]["sections"];
    assert_eq!(sections["inbox"].as_array().map(Vec::len), Some(0));
    assert_eq!(sections["scope_moves"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        sections["near_synonyms"]["contradictions"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    // person 组内容保留：近义组仍在
    let syns = sections["near_synonyms"]["synonyms"].as_array().unwrap();
    assert_eq!(syns.len(), 1, "{syns:?}");
    // 全局计数不受过滤影响（FR-4.4 警告是全局的）
    assert_eq!(v["data"]["inbox_pending"].as_i64(), Some(1));
}
