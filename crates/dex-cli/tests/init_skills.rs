//! `dex init` / `dex skills` 集成测试（FR-6.9 / FR-11.6 / §8.1）。
//!
//! 口径：assert_cmd 无 TTY ⇒ 走 pre-auth 非交互路径，须 `--client human` + DEX_TOKEN；
//! skills 只依赖 HOME（不依赖 DEX_ROOT）；init 目标经 `--path` 或 root 解析。
//! clap 层注意：`CommonOpts` 展平在 `skills` 层 ⇒ 公共参数（--client/--json）须置于
//! 子命令（install/uninstall）**之前**。

mod common;

use assert_cmd::Command;
use common::{Env, HUMAN_TOKEN};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

const SKELETON_DIRS: [&str; 8] = [
    "person", "domains", "apps", "projects", "journal", "inbox", "archive", "index",
];
const SKILLS: [&str; 4] = [
    "dex-bootstrap",
    "dex-propose",
    "dex-review",
    "repo-knowledge",
];

fn tool_dir(home: &Path, tool: &str) -> PathBuf {
    match tool {
        "claude" => home.join(".claude").join("skills"),
        "pi" => home.join(".pi").join("agent").join("skills"),
        "zcode" => home.join(".zcode").join("skills"),
        _ => panic!("未知工具 {tool}"),
    }
}

fn mat_dir(home: &Path) -> PathBuf {
    home.join(".local").join("share").join("dex").join("skills")
}

fn git_log_last(root: &Path) -> String {
    let out = StdCommand::new("git")
        .args(["-C", &root.to_string_lossy(), "log", "-1", "--pretty=%s"])
        .output()
        .expect("git log");
    assert!(
        out.status.success(),
        "git log 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// 跑 dex 并断言退出码，解析 stdout JSON 信封
fn run_json(mut cmd: Command, want_code: i32) -> Value {
    let out = cmd.assert().code(want_code).get_output().clone();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("stdout 非 JSON（{e}）：{text}"))
}

fn has_warning(v: &Value, code: &str) -> bool {
    v["warnings"]
        .as_array()
        .is_some_and(|ws| ws.iter().any(|w| w["code"] == code))
}

fn assert_skeleton(root: &Path) {
    for d in SKELETON_DIRS {
        assert!(root.join(d).is_dir(), "缺骨架目录 {d}");
    }
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).unwrap_or_default(),
        ".cache/\n.obsidian/\n"
    );
}

// ---------- dex init ----------

#[test]
fn init_fresh_path_creates_skeleton_and_first_commit() {
    let env = Env::new();
    let target = TempDir::new().unwrap();
    let root = target.path().join("fresh");
    let root_str = root.to_string_lossy().to_string();

    let v = run_json(
        env.admin(&["init", "--client", "human", "--path", &root_str, "--json"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(v["data"]["root"], serde_json::json!(root_str));
    assert_eq!(v["data"]["git_initialized"], true);
    assert_eq!(v["data"]["created"].as_array().map(Vec::len), Some(9));

    assert_skeleton(&root);
    assert!(root.join(".git").exists(), "应建 git 仓库");
    assert_eq!(git_log_last(&root), "init: dex skeleton");
}

#[test]
fn init_already_complete_returns_8() {
    let env = Env::new();
    let v = run_json(env.admin(&["init", "--client", "human", "--json"]), 8);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "E_REPO_STATE");
}

#[test]
fn init_partial_state_refills_and_commits() {
    // 半成品仓库：八大目录齐但 .gitignore 从未存在（缺 git 可见物）→ 补齐应产生提交
    let env = Env::new();
    let target = TempDir::new().unwrap();
    let root = target.path().join("half");
    for d in SKELETON_DIRS {
        fs::create_dir_all(root.join(d)).unwrap();
    }
    dex_store::git::init_and_first_commit(&root, "fixture: half repo").unwrap();
    fs::remove_dir_all(root.join("journal")).unwrap(); // 叠加目录缺失

    let root_str = root.to_string_lossy().to_string();
    let v = run_json(
        env.admin(&["init", "--client", "human", "--path", &root_str, "--json"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(
        v["data"]["created"],
        serde_json::json!(["journal", ".gitignore"]),
        "只补缺失项：{v}"
    );
    assert_skeleton(&root);
    assert_eq!(git_log_last(&root), "init: 补齐骨架");
}

#[test]
fn init_refill_of_git_invisible_items_tolerates_nothing_to_commit() {
    let env = Env::new();
    // 空目录 git 不追踪：删除后补齐，commit_all 应走 NothingToCommit 容忍（不新增提交）
    fs::remove_dir_all(env.path("inbox")).unwrap();

    let v = run_json(env.admin(&["init", "--client", "human", "--json"]), 0);
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(v["data"]["created"], serde_json::json!(["inbox"]));
    assert!(env.path("inbox").is_dir());
    assert_eq!(
        git_log_last(&env.root),
        "init: test fixture",
        "不应产生新提交"
    );
}

#[test]
fn init_path_supports_tilde_expansion() {
    let env = Env::new();
    let v = run_json(
        env.admin(&[
            "init",
            "--client",
            "human",
            "--path",
            "~/tilde-dex",
            "--json",
        ]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    let root = env.home.join("tilde-dex");
    assert_skeleton(&root);
    assert_eq!(git_log_last(&root), "init: dex skeleton");
}

#[test]
fn init_git_missing_degrades_with_warning() {
    let env = Env::new();
    let target = TempDir::new().unwrap();
    let root = target.path().join("nogit");
    let root_str = root.to_string_lossy().to_string();

    let mut cmd = env.admin(&["init", "--client", "human", "--path", &root_str]);
    let out = cmd.env("PATH", "").assert().code(0).get_output().clone();
    // text 模式：骨架照建 + W_GIT_UNAVAILABLE 走 stderr，退出码 0
    assert_skeleton(&root);
    assert!(!root.join(".git").exists(), "git 缺失时不应建仓库");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("W_GIT_UNAVAILABLE"),
        "stderr 应含降级警告：{stderr}"
    );

    // json 模式同口径：warning 进信封
    let target2 = TempDir::new().unwrap();
    let root2 = target2.path().join("nogit2");
    let root2_str = root2.to_string_lossy().to_string();
    let mut cmd2 = env.admin(&["init", "--client", "human", "--path", &root2_str, "--json"]);
    cmd2.env("PATH", "");
    let v = run_json(cmd2, 0);
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["git_initialized"], false);
    assert!(has_warning(&v, "W_GIT_UNAVAILABLE"), "信封应含警告：{v}");
}

/// 自建隔离环境（不依赖 Env 夹具，可自由控制 DEX_ROOT 的有无）；返回 home 供断言
fn bare_env(dir: &TempDir, dex_root: Option<&Path>) -> (PathBuf, Command) {
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let mut cmd = Command::cargo_bin("dex").unwrap();
    cmd.args(["init", "--client", "human", "--json"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("DEX_TOKEN", HUMAN_TOKEN);
    match dex_root {
        Some(r) => {
            cmd.env("DEX_ROOT", r);
        }
        None => {
            cmd.env_remove("DEX_ROOT");
        }
    };
    (home, cmd)
}

#[test]
fn init_default_root_resolves_dex_root_env() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("dexrepo");
    let (_home, cmd) = bare_env(&dir, Some(&root));
    let v = run_json(cmd, 0);
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(
        v["data"]["root"],
        serde_json::json!(root.to_string_lossy().to_string())
    );
    assert_skeleton(&root);
    assert_eq!(git_log_last(&root), "init: dex skeleton");
}

#[test]
fn init_default_root_falls_back_to_home_dex() {
    let dir = TempDir::new().unwrap();
    let (home, cmd) = bare_env(&dir, None);
    let v = run_json(cmd, 0);
    assert_eq!(v["ok"], true, "信封：{v}");
    let root = home.join("dex");
    assert_eq!(
        v["data"]["root"],
        serde_json::json!(root.to_string_lossy().to_string()),
        "无 DEX_ROOT / 本机层 root 时应落 ~/dex"
    );
    assert_skeleton(&root);
    assert_eq!(git_log_last(&root), "init: dex skeleton");
}

// ---------- dex skills ----------

#[test]
fn skills_install_embedded_all_tools() {
    let env = Env::new();
    let v = run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(v["data"]["from"], "embedded");
    let mat = mat_dir(&env.home);
    assert_eq!(
        v["data"]["materialize_dir"],
        serde_json::json!(mat.to_string_lossy().to_string())
    );

    // 物化目录含五项 + SKILL.md
    assert!(mat.join("dex-bootstrap/SKILL.md").is_file());
    assert!(mat.join("dex-propose/SKILL.md").is_file());
    assert!(mat.join("dex-review/SKILL.md").is_file());
    assert!(mat.join("repo-knowledge/SKILL.md").is_file());
    assert!(mat.join("connectors").is_dir());

    // 三工具 × 四技能 symlink 指向物化路径
    for tool in ["claude", "pi", "zcode"] {
        for skill in SKILLS {
            let link = tool_dir(&env.home, tool).join(skill);
            let meta = fs::symlink_metadata(&link).unwrap_or_else(|e| panic!("{link:?}: {e}"));
            assert!(meta.file_type().is_symlink(), "{link:?} 应为 symlink");
            assert_eq!(
                fs::read_link(&link).unwrap(),
                mat.join(skill),
                "{tool}/{skill} 指向错误"
            );
            assert!(
                link.join("SKILL.md").is_file(),
                "{link:?} 应可解析到 SKILL.md"
            );
        }
        assert_eq!(
            v["data"]["installed"][tool].as_array().map(Vec::len),
            Some(4),
            "{tool} 应装四技能：{v}"
        );
    }
}

#[test]
fn skills_install_is_idempotent_skip() {
    let env = Env::new();
    run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    let mat = mat_dir(&env.home);
    let v = run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    for tool in ["claude", "pi", "zcode"] {
        assert_eq!(
            v["data"]["installed"][tool].as_array().map(Vec::len),
            Some(0),
            "重跑不应再装：{v}"
        );
        assert_eq!(
            v["data"]["skipped"][tool].as_array().map(Vec::len),
            Some(4),
            "应全部 skip：{v}"
        );
    }
    let link = tool_dir(&env.home, "claude").join("dex-bootstrap");
    assert_eq!(fs::read_link(&link).unwrap(), mat.join("dex-bootstrap"));
    assert!(link.join("SKILL.md").is_file());
}

#[test]
fn skills_install_rebuilds_dead_links() {
    let env = Env::new();
    run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    let mat = mat_dir(&env.home);
    // 换成指向不存在处的死链 → 应删了重建
    let link = tool_dir(&env.home, "pi").join("dex-propose");
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("/nonexistent/dex-propose", &link).unwrap();

    let v = run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(
        v["data"]["installed"]["pi"],
        serde_json::json!(["dex-propose"]),
        "死链应重装：{v}"
    );
    assert_eq!(fs::read_link(&link).unwrap(), mat.join("dex-propose"));
    assert!(link.join("SKILL.md").is_file());
}

#[test]
fn skills_install_remateralizes_after_wipe() {
    let env = Env::new();
    run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    let mat = mat_dir(&env.home);
    fs::remove_dir_all(&mat).unwrap();
    let v = run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    assert!(
        mat.join("dex-bootstrap/SKILL.md").is_file(),
        "物化目录应重建"
    );
    // 物化重建后既有 symlink 复活（幂等 skip 路径）
    let link = tool_dir(&env.home, "zcode").join("dex-review");
    assert!(link.join("SKILL.md").is_file(), "symlink 应重新可用");
}

#[test]
fn skills_install_conflict_entity_refuses_10() {
    let env = Env::new();
    let claude = tool_dir(&env.home, "claude");
    fs::create_dir_all(claude.join("dex-bootstrap")).unwrap();
    fs::write(claude.join("dex-bootstrap/SKILL.md"), "user's own skill").unwrap();

    let v = run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        10,
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "E_RENDER_REFUSE", "信封：{v}");
    // 非本仓条目不被覆盖
    assert_eq!(
        fs::read_to_string(claude.join("dex-bootstrap/SKILL.md")).unwrap(),
        "user's own skill"
    );
}

#[test]
fn skills_install_tool_subset_and_unknown_tool() {
    let env = Env::new();
    let v = run_json(
        env.admin(&[
            "skills", "--client", "human", "--json", "install", "--tool", "claude",
        ]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    let claude_link = tool_dir(&env.home, "claude").join("dex-bootstrap");
    let meta = fs::symlink_metadata(&claude_link).unwrap();
    assert!(
        meta.file_type().is_symlink(),
        "所选工具应已 symlink：{claude_link:?}"
    );
    assert!(!tool_dir(&env.home, "pi").exists(), "未选工具不应建目录");
    assert!(!tool_dir(&env.home, "zcode").exists(), "未选工具不应建目录");

    let v2 = run_json(
        env.admin(&[
            "skills", "--client", "human", "--json", "install", "--tool", "foo",
        ]),
        2,
    );
    assert_eq!(v2["ok"], false);
    assert_eq!(v2["error"]["code"], "E_BAD_ARGS");
}

#[test]
fn skills_install_from_working_copy() {
    let env = Env::new();
    let src = TempDir::new().unwrap();
    for skill in SKILLS {
        fs::create_dir_all(src.path().join(skill)).unwrap();
        fs::write(
            src.path().join(skill).join("SKILL.md"),
            format!("MARKER-FROM {skill}"),
        )
        .unwrap();
    }
    fs::create_dir_all(src.path().join("connectors")).unwrap();
    fs::write(
        src.path().join("connectors/README.md"),
        "connectors from copy",
    )
    .unwrap();
    let src_str = src.path().to_string_lossy().to_string();

    let v = run_json(
        env.admin(&[
            "skills", "--client", "human", "--json", "install", "--from", &src_str,
        ]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(v["data"]["from"], serde_json::json!(src_str));
    let mat = mat_dir(&env.home);
    assert_eq!(
        fs::read_to_string(mat.join("dex-propose/SKILL.md")).unwrap(),
        "MARKER-FROM dex-propose",
        "物化内容应来自 --from 副本"
    );
    assert!(mat.join("connectors/README.md").is_file());

    // --from 无 dex-*/SKILL.md → E_BAD_ARGS · 2
    let empty = TempDir::new().unwrap();
    fs::create_dir_all(empty.path().join("not-a-skill")).unwrap();
    let v2 = run_json(
        env.admin(&[
            "skills",
            "--client",
            "human",
            "--json",
            "install",
            "--from",
            &empty.path().to_string_lossy(),
        ]),
        2,
    );
    assert_eq!(v2["error"]["code"], "E_BAD_ARGS");
}

#[test]
fn skills_uninstall_removes_only_ours() {
    let env = Env::new();
    run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    let mat = mat_dir(&env.home);
    // zcode/dex-review 换成指向别处的 symlink（非本仓）→ 留置标注
    let zdir = tool_dir(&env.home, "zcode");
    let foreign = TempDir::new().unwrap();
    fs::remove_file(zdir.join("dex-review")).unwrap();
    std::os::unix::fs::symlink(foreign.path(), zdir.join("dex-review")).unwrap();

    let v = run_json(
        env.admin(&["skills", "--client", "human", "--json", "uninstall"]),
        0,
    );
    assert_eq!(v["ok"], true, "信封：{v}");
    for tool in ["claude", "pi"] {
        assert_eq!(
            v["data"]["removed"][tool].as_array().map(Vec::len),
            Some(4),
            "{tool} 应移除四技能：{v}"
        );
        for skill in SKILLS {
            assert!(
                fs::symlink_metadata(tool_dir(&env.home, tool).join(skill)).is_err(),
                "{tool}/{skill} symlink 应已移除"
            );
        }
    }
    // 非本仓 symlink 留置 + 实体目录留置（不可见条目不报错）
    assert_eq!(
        v["data"]["conflicts"]["zcode"],
        serde_json::json!(["dex-review"]),
        "非本仓条目应标注 conflict：{v}"
    );
    assert_eq!(
        fs::read_link(zdir.join("dex-review")).unwrap(),
        foreign.path()
    );
    // 物化目录不动
    assert!(
        mat.join("dex-bootstrap/SKILL.md").is_file(),
        "物化目录应保留"
    );
}

#[test]
fn skills_status_lists_states() {
    let env = Env::new();
    // 初始：全部 missing
    let v = run_json(env.admin(&["skills", "--client", "human", "--json"]), 0);
    assert_eq!(v["ok"], true, "信封：{v}");
    assert_eq!(
        v["data"]["materialize_dir"],
        serde_json::json!(mat_dir(&env.home).to_string_lossy().to_string())
    );
    for tool in ["claude", "pi", "zcode"] {
        for skill in SKILLS {
            assert_eq!(
                v["data"]["tools"][tool][skill], "missing",
                "初始应 missing：{v}"
            );
        }
    }

    // 安装后：installed；实体占用 → conflict
    run_json(
        env.admin(&["skills", "--client", "human", "--json", "install"]),
        0,
    );
    let pidir = tool_dir(&env.home, "pi");
    fs::remove_file(pidir.join("dex-bootstrap")).unwrap();
    fs::create_dir_all(pidir.join("dex-bootstrap")).unwrap();

    let v2 = run_json(env.admin(&["skills", "--client", "human", "--json"]), 0);
    assert_eq!(v2["data"]["tools"]["claude"]["dex-bootstrap"], "installed");
    assert_eq!(v2["data"]["tools"]["pi"]["dex-bootstrap"], "conflict");
    assert_eq!(v2["data"]["tools"]["pi"]["dex-propose"], "installed");
    assert_eq!(v2["data"]["tools"]["zcode"]["dex-review"], "installed");
}
