//! search / read 集成测试（D-a）：
//! - search 退出码 0/1/2/3 各至少一例；缺省检索集按白名单收敛、--limit clamp、json 信封形状
//! - read 退出码 0/1/3/6 各至少一例；`..`/绝对路径/`.git`/`.dex` 禁区/symlink 逃逸、
//!   journal 显式授权面（human 可读、choose-you 拒绝）、--section 抽取
//! - 审计断言：`.cache/audit.log`（dex_store::audit::read_all）含拒绝行且不含内容/token

mod common;

use common::{Env, HUMAN_TOKEN, SPOKE_TOKEN};
use predicates::str::contains;
use serde_json::{json, Value};

/// choose-you 客户端（scopes = person + apps/todo）调用快捷方式
fn choose_you(env: &Env, args: &[&str]) -> assert_cmd::Command {
    env.dex_with_token(args, Some(SPOKE_TOKEN))
}

fn stdout_of(mut cmd: assert_cmd::Command) -> String {
    let out = cmd.assert().get_output().stdout.clone();
    String::from_utf8(out).expect("utf8 stdout")
}

fn audit_log(env: &Env) -> String {
    dex_store::audit::read_all(&env.root).join("\n")
}

// ---------- search ----------

/// 缺省检索集 = 白名单 ∩ 四层：choose-you 命中 person + apps/todo，
/// 不命中白名单外的 domains/coding（即使 query 匹配其内容）
#[test]
fn search_default_set_hits_whitelist_layers_only() {
    let env = Env::new();
    env.standard_config();
    let s = stdout_of(choose_you(
        &env,
        &["search", "--client", "choose-you", "深度|周四|蓝绿"],
    ));
    assert_eq!(
        stdout_of(choose_you(
            &env,
            &["search", "--client", "choose-you", "深度|周四|蓝绿"]
        )),
        s // 确定性：两次调用输出一致
    );
    assert!(
        s.contains("person/profile.md:4:person:- 深度工作时段不接打断"),
        "应命中 person：{s}"
    );
    assert!(
        s.contains("apps/todo/rules.md:2:apps/todo:- 周报类任务多在周四下午被提到"),
        "应命中 apps/todo：{s}"
    );
    assert!(
        !s.contains("domains/coding"),
        "缺省集不得越权检索 domains：{s}"
    );
    assert!(!s.contains("蓝绿"), "越权内容不得泄漏：{s}");
}

/// 显式 --scope 越权 → 3 + audit.log 有拒绝行（不含内容/token）
#[test]
fn search_explicit_scope_denied_writes_audit() {
    let env = Env::new();
    env.standard_config();
    choose_you(
        &env,
        &[
            "search",
            "--client",
            "choose-you",
            "--scope",
            "domains/coding",
            "蓝绿",
        ],
    )
    .assert()
    .code(3)
    .stderr(contains("超出客户端白名单"));
    let log = audit_log(&env);
    assert!(
        log.contains("choose-you|search|domains/coding|E_SCOPE_DENIED"),
        "audit 应有拒绝行：{log}"
    );
    assert!(!log.contains(SPOKE_TOKEN), "audit 不得记 token：{log}");
    assert!(!log.contains("蓝绿"), "audit 不得记检索词/内容：{log}");
}

/// 未知 scope → E_BAD_ARGS · 2；坏正则 → E_BAD_ARGS · 2
#[test]
fn search_bad_args_exit_2() {
    let env = Env::new();
    env.standard_config();
    choose_you(
        &env,
        &["search", "--client", "choose-you", "--scope", "nope", "x"],
    )
    .assert()
    .code(2)
    .stderr(contains("未知 scope"));
    choose_you(&env, &["search", "--client", "choose-you", "[unclosed"])
        .assert()
        .code(2)
        .stderr(contains("正则"));
}

/// --limit：缺省 10；>20 clamp 20；0 → E_BAD_ARGS · 2
#[test]
fn search_limit_default_and_clamp() {
    let env = Env::new();
    env.standard_config();
    let mut bulk = String::from("# Bulk\n");
    for i in 1..=30 {
        bulk.push_str(&format!("- bulk 行 {i}\n"));
    }
    std::fs::write(env.path("person/bulk.md"), bulk).unwrap();

    let count_lines = |s: &str| s.lines().filter(|l| !l.is_empty()).count();
    let default = stdout_of(choose_you(
        &env,
        &["search", "--client", "choose-you", "bulk 行"],
    ));
    assert_eq!(count_lines(&default), 10, "缺省 limit = 10：{default}");

    let clamped = stdout_of(choose_you(
        &env,
        &[
            "search",
            "--client",
            "choose-you",
            "--limit",
            "99",
            "bulk 行",
        ],
    ));
    assert_eq!(
        count_lines(&clamped),
        20,
        "--limit 99 应 clamp 到 20：{clamped}"
    );

    choose_you(
        &env,
        &[
            "search",
            "--client",
            "choose-you",
            "--limit",
            "0",
            "bulk 行",
        ],
    )
    .assert()
    .code(2)
    .stderr(contains("--limit"));
}

/// 无命中 → E_NOT_FOUND · 1（text stderr + json 错误信封 ok:false）
#[test]
fn search_no_hits_exit_1() {
    let env = Env::new();
    env.standard_config();
    env.admin(&["search", "--client", "human", "zzz_no_such_term"])
        .assert()
        .code(1)
        .stderr(contains("search 无命中"));

    let out = stdout_of(env.admin(&["search", "--client", "human", "--json", "zzz_no_such_term"]));
    let v: Value = serde_json::from_str(&out).expect("json 信封");
    assert_eq!(v["ok"], json!(false));
    assert_eq!(v["error"]["code"], json!("E_NOT_FOUND"));
}

/// --json 成功信封形状：{ok, warnings, data:{query, count, results[{path,line,scope,content}]}}
#[test]
fn search_json_envelope_shape() {
    let env = Env::new();
    env.standard_config();
    let out = stdout_of(choose_you(
        &env,
        &["search", "--client", "choose-you", "--json", "周报"],
    ));
    let v: Value = serde_json::from_str(&out).expect("json 信封");
    assert_eq!(v["ok"], json!(true));
    assert!(v["warnings"].is_array(), "warnings 恒为数组：{out}");
    assert_eq!(v["data"]["query"], json!("周报"));
    assert_eq!(v["data"]["count"], json!(1));
    let r = &v["data"]["results"][0];
    assert_eq!(r["path"], json!("apps/todo/rules.md"));
    assert_eq!(r["line"], json!(2));
    assert_eq!(r["scope"], json!("apps/todo"));
    assert!(r["content"].as_str().unwrap().contains("周报类任务"));
}

// ---------- read ----------

/// 正常读：text 首行 `# scope: <scope>` 后接原始内容
#[test]
fn read_ok_with_scope_header() {
    let env = Env::new();
    env.standard_config();
    let s = stdout_of(choose_you(
        &env,
        &["read", "--client", "choose-you", "person/profile.md"],
    ));
    assert!(s.starts_with("# scope: person\n"), "首行 scope 标注：{s}");
    assert!(s.contains("- 主业是个人工具开发"), "原始内容：{s}");
}

/// `..` 段 / 绝对路径 → E_BAD_PATH · 6（audit）
#[test]
fn read_rejects_traversal_and_absolute() {
    let env = Env::new();
    env.standard_config();
    choose_you(&env, &["read", "--client", "choose-you", "../secret.md"])
        .assert()
        .code(6)
        .stderr(contains("路径非法"));
    choose_you(&env, &["read", "--client", "choose-you", "/etc/passwd"])
        .assert()
        .code(6);
    let log = audit_log(&env);
    assert!(
        log.contains("choose-you|read|../secret.md|E_BAD_PATH"),
        "audit 应有拒绝行：{log}"
    );
}

/// 首段禁区 `.git` / `.dex` → E_BAD_PATH · 6（audit；先于存在性判定）
#[test]
fn read_rejects_forbidden_zones() {
    let env = Env::new();
    env.standard_config();
    choose_you(&env, &["read", "--client", "choose-you", ".git/config"])
        .assert()
        .code(6);
    choose_you(
        &env,
        &["read", "--client", "choose-you", ".dex/config.toml"],
    )
    .assert()
    .code(6);
    let log = audit_log(&env);
    assert!(log.contains("|.git/config|E_BAD_PATH"), "{log}");
    assert!(log.contains("|.dex/config.toml|E_BAD_PATH"), "{log}");
}

/// symlink 逃逸出 DEX_ROOT → E_BAD_PATH · 6（audit）
#[cfg(unix)]
#[test]
fn read_rejects_symlink_escape() {
    let env = Env::new();
    env.standard_config();
    std::fs::write(env.home.join("outside.md"), "# 根外文件\n").unwrap();
    std::os::unix::fs::symlink(env.home.join("outside.md"), env.path("person/escape.md")).unwrap();
    choose_you(
        &env,
        &["read", "--client", "choose-you", "person/escape.md"],
    )
    .assert()
    .code(6)
    .stderr(contains("路径非法"));
    let log = audit_log(&env);
    assert!(
        log.contains("|person/escape.md|E_BAD_PATH"),
        "audit 应有拒绝行：{log}"
    );
}

/// 越权读（choose-you 读 domains/coding）→ E_SCOPE_DENIED · 3（audit；stdout 无内容）
#[test]
fn read_scope_denied_writes_audit() {
    let env = Env::new();
    env.standard_config();
    let mut cmd = choose_you(
        &env,
        &["read", "--client", "choose-you", "domains/coding/deploy.md"],
    );
    let stdout = cmd
        .assert()
        .code(3)
        .stderr(contains("scope 拒绝"))
        .get_output()
        .stdout
        .clone();
    assert!(
        stdout.is_empty(),
        "越权读不得输出任何内容：{:?}",
        String::from_utf8_lossy(&stdout)
    );
    let log = audit_log(&env);
    assert!(
        log.contains("choose-you|read|domains/coding/deploy.md|E_SCOPE_DENIED"),
        "audit 应有拒绝行：{log}"
    );
    assert!(
        !log.contains(SPOKE_TOKEN) && !log.contains("蓝绿"),
        "audit 不得含 token/内容：{log}"
    );
    assert!(!log.contains(HUMAN_TOKEN), "audit 不得含 token：{log}");
}

/// 文件不存在 → E_NOT_FOUND · 1
#[test]
fn read_not_found_exit_1() {
    let env = Env::new();
    env.standard_config();
    choose_you(&env, &["read", "--client", "choose-you", "person/ghost.md"])
        .assert()
        .code(1)
        .stderr(contains("文件不存在"));
}

/// --section：抽取 `## <title>` 至下一 `## `；标题找不到 → E_NOT_FOUND · 1；json 形状
#[test]
fn read_section_extraction() {
    let env = Env::new();
    env.standard_config();
    std::fs::write(
        env.path("person/sections.md"),
        "# 页\n\n## Alpha\n\n- alpha 甲\n- alpha 乙\n\n## Beta\n\n- beta 丙\n",
    )
    .unwrap();

    let s = stdout_of(choose_you(
        &env,
        &[
            "read",
            "--client",
            "choose-you",
            "--section",
            "Alpha",
            "person/sections.md",
        ],
    ));
    assert!(s.starts_with("# scope: person\n"), "首行 scope 标注：{s}");
    assert!(s.contains("## Alpha"), "小节含标题行：{s}");
    assert!(s.contains("- alpha 甲") && s.contains("- alpha 乙"), "{s}");
    assert!(!s.contains("beta"), "不得泄漏后续小节：{s}");

    let out = stdout_of(choose_you(
        &env,
        &[
            "read",
            "--client",
            "choose-you",
            "--json",
            "--section",
            "Alpha",
            "person/sections.md",
        ],
    ));
    let v: Value = serde_json::from_str(&out).expect("json 信封");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["data"]["path"], json!("person/sections.md"));
    assert_eq!(v["data"]["scope"], json!("person"));
    assert_eq!(v["data"]["section"], json!("Alpha"));
    assert!(v["data"]["content"]
        .as_str()
        .unwrap()
        .contains("- alpha 甲"));

    choose_you(
        &env,
        &[
            "read",
            "--client",
            "choose-you",
            "--section",
            "Ghost",
            "person/sections.md",
        ],
    )
    .assert()
    .code(1)
    .stderr(contains("小节不存在"));
}

/// journal 显式授权面（FR-3.1）：human 全量可读；choose-you 白名单不含 journal → 3
#[test]
fn read_journal_explicit_grant_face() {
    let env = Env::new();
    env.standard_config();
    std::fs::write(
        env.path("journal/2026-09-25.md"),
        "# 2026-09-25\n\n## 手写\n\n- 今天做了检索测试\n",
    )
    .unwrap();

    let s = stdout_of(env.admin(&["read", "--client", "human", "journal/2026-09-25.md"]));
    assert!(
        s.starts_with("# scope: journal\n"),
        "human 可读 journal：{s}"
    );
    assert!(s.contains("- 今天做了检索测试"), "{s}");

    choose_you(
        &env,
        &["read", "--client", "choose-you", "journal/2026-09-25.md"],
    )
    .assert()
    .code(3);
}
