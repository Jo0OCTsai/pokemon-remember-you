//! dex-cli 集成测试共享夹具（D 组共用；勿改签名，可加助手）。
//!
//! 关键口径：
//! - assert_cmd 无 TTY ⇒ 走**非交互路径**：一切调用须 `--client <id>` + 凭证
//!   （DEX_TOKEN 环境变量覆盖即可，≥32 字符）
//! - 环境隔离：HOME / XDG_CONFIG_HOME 指向临时家目录，DEX_ROOT 指向临时图鉴仓库
//! - human 的管理命令在测试中 = `--client human` + DEX_TOKEN（非 TTY 下需要凭证，FR-10.3）
#![allow(dead_code)] // 各测试二进制按需取用助手，未用项不应阻断编译

use assert_cmd::Command;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const HUMAN_TOKEN: &str = "dex_test_human_token_0123456789abcdef0123456789abcdef";
pub const SPOKE_TOKEN: &str = "dex_test_spoke_token_0123456789abcdef0123456789abcdef";

/// 临时环境：家目录 + 图鉴仓库（八大目录骨架 + 初始提交）
pub struct Env {
    pub home: PathBuf,
    pub root: PathBuf,
    _dir: TempDir, // 保持存活
}

impl Env {
    /// 建图鉴仓库（person/domains/apps/projects 各带样例文件）+ git 初始提交
    pub fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let home = dir.path().join("home");
        let root = dir.path().join("dex");
        fs::create_dir_all(&home).unwrap();
        for d in [
            "person",
            "domains/coding",
            "apps/todo",
            "projects/foo",
            "journal",
            "inbox",
            "archive",
            "index",
        ] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        fs::write(root.join(".gitignore"), ".cache/\n.obsidian/\n").unwrap();
        fs::write(
            root.join("person/profile.md"),
            "# 个人画像\n<!-- src: bootstrap-interview 2026-09-22 -->\n- 主业是个人工具开发\n- 深度工作时段不接打断\n",
        )
        .unwrap();
        fs::write(
            root.join("person/preferences.md"),
            "# 偏好\n- 中文写作避免「进行」「予以」一类冗词\n<!-- keep-until: 2099-12-31 长期偏好 -->\n",
        )
        .unwrap();
        fs::write(
            root.join("domains/coding/deploy.md"),
            "# 部署\n<!-- src: choose-you 固化 2026-09 · 证据×6 -->\n- 部署走蓝绿而不是直接覆盖\n- 提交前必跑 lint 与单测\n",
        )
        .unwrap();
        fs::write(
            root.join("apps/todo/rules.md"),
            "# 待办规则\n- 周报类任务多在周四下午被提到\n",
        )
        .unwrap();
        fs::write(
            root.join("projects/foo/notes.md"),
            "# 项目备忘\n<!-- src: harvest ~/.claude/projects/foo/memory -->\n- owner 是李四、周五不发布\n",
        )
        .unwrap();
        dex_store::git::init_and_first_commit(&root, "init: test fixture").expect("git init");
        Env {
            home,
            root,
            _dir: dir,
        }
    }

    /// 写本机层 config（~/.config/dex/config.toml）
    pub fn machine_config(&self, toml_text: &str) {
        let cfg = self.home.join(".config/dex");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(cfg.join("config.toml"), toml_text).unwrap();
    }

    /// 写本机凭据文件（0600）：clients.<id>.token
    pub fn credentials(&self, tokens: &[(&str, &str)]) {
        let cfg = self.home.join(".config/dex");
        fs::create_dir_all(&cfg).unwrap();
        let mut text = String::new();
        for (id, token) in tokens {
            text.push_str(&format!("[clients.\"{id}\"]\ntoken = \"{token}\"\n"));
        }
        let path = cfg.join("credentials.toml");
        fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    /// 标准机器 config：human + choose-you + work-agent 三个客户端
    pub fn standard_config(&self) {
        self.machine_config(&format!(
            r#"root = "{root}"

[clients."choose-you"]
scopes = ["person", "apps/todo"]
propose = true
rate_limit = {{ proposals_per_day = 20, journal_per_day = 60 }}
allowed_sources = ["choose-you"]

[clients."work-agent"]
scopes = ["person", "domains/coding"]
allowed_sources = ["work-agent"]

[harvest.sources."im-x"]
type = "page"
first_batch = 30

[clients."harvest-im-x"]
scopes = []
propose = true
allowed_sources = ["im-x"]
"#,
            root = self.root.display()
        ));
        self.credentials(&[
            ("human", HUMAN_TOKEN),
            ("choose-you", SPOKE_TOKEN),
            ("harvest-im-x", SPOKE_TOKEN),
        ]);
    }

    /// 起一条 dex 调用（非交互：须带 --client；token 经 DEX_TOKEN 注入）
    pub fn dex(&self, args: &[&str]) -> Command {
        self.dex_with_token(args, None)
    }

    /// `--client human` 的管理命令快捷方式（非 TTY 下 human 管理命令须显式 --client + 凭证）
    pub fn admin(&self, args: &[&str]) -> Command {
        let mut full: Vec<&str> = args.to_vec();
        if !full.contains(&"--client") {
            full.push("--client");
            full.push("human");
        }
        self.dex_with_token(&full, Some(HUMAN_TOKEN))
    }

    pub fn dex_with_token(&self, args: &[&str], token: Option<&str>) -> Command {
        let mut cmd = Command::cargo_bin("dex").expect("cargo bin dex");
        cmd.args(args)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("DEX_ROOT", &self.root)
            .env_remove("DEX_TOKEN");
        if let Some(t) = token {
            cmd.env("DEX_TOKEN", t);
        }
        cmd
    }

    /// 图鉴仓库内相对路径的绝对路径
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

/// 消费方工作区（独立 git 仓库）：render 的 out 路径基准 = cwd 所在 git 仓库根
pub struct Workspace {
    pub dir: PathBuf,
    _dir: TempDir,
}

impl Workspace {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let ws = dir.path().join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("README.md"), "# ws\n").unwrap();
        dex_store::git::init_and_first_commit(&ws, "init: workspace").expect("git init ws");
        Workspace { dir: ws, _dir: dir }
    }
}

/// 等待 git 提交在文件系统可见（测试内通常无需等待，保留给竞态敏感用例）
pub fn commit_dates(root: &Path) -> BTreeMap<String, String> {
    // 框架缺口 workaround（D-e）：last_touch_snapshot 返回 HashMap，此处按声明签名收敛为 BTreeMap
    dex_store::git::last_touch_snapshot(root)
        .unwrap_or_default()
        .into_iter()
        .collect()
}
