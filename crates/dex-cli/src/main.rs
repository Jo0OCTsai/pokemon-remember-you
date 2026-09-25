//! dex CLI 入口（DESIGN §7.1 主流程 / §8.1 命令面装配）。
//!
//! 装配序：解析参数 → 定位仓库（DEX_ROOT → config.root → ~/dex）→ 加载两层 config
//! → 解析客户端身份（TTY human / --client + 凭证，FR-10.3）→ 权限分档（FR-10.4）
//! → 分发命令。init/skills 为仓库前置命令（pre-auth 路径）。

mod cmd;
mod config;
mod guard_runtime;
mod output;

use clap::{Parser, Subcommand};
use cmd::{CommonOpts, JsonClientOpts};
use dex_core::DexError;
use guard_runtime::{check_class, resolve_client, CommandClass, Ctx};

#[derive(Parser, Debug)]
#[command(
    name = "dex",
    version,
    about = "个人记忆中枢（pokemon-remember-you）CLI",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 全文检索（FR-6.1）：path:line:scope:content
    Search(cmd::search::SearchArgs),
    /// 读单文件/小节，带 scope 标注（FR-6.2）
    Read(cmd::read::ReadArgs),
    /// 提交提案到 inbox（FR-6.3，守卫 0–8）
    Propose(cmd::propose::ProposeArgs),
    /// journal 供稿（FR-5.4，守卫子集）
    Journal(cmd::journal::JournalArgs),
    /// 渲染入口文件（FR-6.4，注入管线 + 预算）
    Render(cmd::render::RenderArgs),
    /// 衰减候选清单（FR-6.5，单遍 git log 口径）
    Stale(cmd::stale::StaleArgs),
    /// 周回顾七段清单（FR-6.6）
    Review(cmd::review::ReviewArgs),
    /// 结构体检（FR-6.11，四层检查集）
    Lint(cmd::lint::LintArgs),
    /// 重建派生索引（v2；v1 存根）
    Reindex(cmd::reindex_stub::ReindexArgs),
    /// 建仓骨架 + git init（FR-6.9）
    Init(cmd::init::InitArgs),
    /// 技能安装管理（FR-11.6）
    Skills(cmd::skills::SkillsArgs),
    /// 收割会话便利封装（FR-6.14，不蒸馏）
    Harvest(cmd::harvest::HarvestArgs),
    /// 渐进式面试草稿提案（FR-6.14）
    Interview(cmd::interview::InterviewArgs),
}

/// 从命令参数提取 (json, client)。`--format` 优先于 `--json`（FR-6.10）；
/// 非法 format 值提前失败（E_BAD_ARGS · 2）。
fn normalize(
    common: (&bool, &Option<String>, &Option<String>),
) -> Result<(bool, Option<String>), DexError> {
    let (json, format, client) = common;
    match format.as_deref() {
        Some("json") => Ok((true, client.clone())),
        Some("text") => Ok((false, client.clone())),
        Some(other) => Err(DexError::BadArgs {
            message: format!("--format 值非法：{other:?}（text|json）"),
        }),
        None => Ok((*json, client.clone())),
    }
}

fn common_of(c: &Commands) -> Result<(bool, Option<String>), DexError> {
    fn plain(c: &CommonOpts) -> Result<(bool, Option<String>), DexError> {
        normalize((&c.json, &c.format, &c.client))
    }
    fn json_only(c: &JsonClientOpts) -> Result<(bool, Option<String>), DexError> {
        // render 的 --format 是产物格式（merged|import），机器可读输出只认 --json
        Ok((c.json, c.client.clone()))
    }
    match c {
        Commands::Search(a) => plain(&a.common),
        Commands::Read(a) => plain(&a.common),
        Commands::Propose(a) => plain(&a.common),
        Commands::Journal(a) => plain(&a.common),
        Commands::Render(a) => json_only(&a.common),
        Commands::Stale(a) => plain(&a.common),
        Commands::Review(a) => plain(&a.common),
        Commands::Lint(a) => plain(&a.common),
        Commands::Reindex(a) => plain(&a.common),
        Commands::Init(a) => plain(&a.common),
        Commands::Skills(a) => plain(&a.common),
        Commands::Harvest(a) => plain(&a.common),
        Commands::Interview(a) => plain(&a.common),
    }
}

fn class_of(c: &Commands) -> CommandClass {
    match c {
        Commands::Search(_) | Commands::Read(_) => CommandClass::Read,
        Commands::Propose(_) | Commands::Journal(_) => CommandClass::Write,
        Commands::Render(_)
        | Commands::Review(_)
        | Commands::Stale(_)
        | Commands::Lint(_)
        | Commands::Reindex(_) => CommandClass::Management,
        Commands::Harvest(_) | Commands::Interview(_) => CommandClass::Harvest,
        Commands::Init(_) | Commands::Skills(_) => CommandClass::Management,
    }
}

fn name_of(c: &Commands) -> &'static str {
    match c {
        Commands::Search(_) => "search",
        Commands::Read(_) => "read",
        Commands::Propose(_) => "propose",
        Commands::Journal(_) => "journal",
        Commands::Render(_) => "render",
        Commands::Stale(_) => "stale",
        Commands::Review(_) => "review",
        Commands::Lint(_) => "lint",
        Commands::Reindex(_) => "reindex",
        Commands::Init(_) => "init",
        Commands::Skills(_) => "skills",
        Commands::Harvest(_) => "harvest",
        Commands::Interview(_) => "interview",
    }
}

fn main() {
    let cli = Cli::parse();
    std::process::exit(dispatch(cli.command));
}

fn fail(json: bool, err: DexError) -> i32 {
    if json {
        output::print_err(&err);
    } else {
        eprintln!("dex: {err}");
    }
    err.exit_code()
}

fn dispatch(command: Commands) -> i32 {
    let (json, client_flag) = match common_of(&command) {
        Ok(x) => x,
        Err(e) => return fail(false, e), // format 非法时尚无可靠 json 信号，按 text 报错
    };

    // 仓库前置命令（init/skills）：pre-auth——TTY human 直行；非交互须完整凭证 + human
    match &command {
        Commands::Init(args) => {
            return run_pre_auth(json, client_flag.as_deref(), "init", || {
                cmd::init::run(args)
            })
        }
        Commands::Skills(args) => {
            return run_pre_auth(json, client_flag.as_deref(), "skills", || {
                cmd::skills::run(args)
            })
        }
        _ => {}
    }

    // 定位仓库 + 加载配置
    let (config, warnings) = match config::load() {
        Ok(x) => x,
        Err(e) => return fail(json, e),
    };
    let root = config.root.clone();
    if !root.is_dir() {
        return fail(
            json,
            DexError::RepoState {
                message: format!(
                    "未找到图鉴仓库 {}——先运行 dex init，或设置 DEX_ROOT 指向既有仓库",
                    root.display()
                ),
            },
        );
    }
    let (creds, cred_warnings) = match config::load_credentials(&config.credentials_file) {
        Ok(x) => x,
        Err(e) => return fail(json, e),
    };
    let mut warnings = warnings;
    warnings.extend(cred_warnings);

    // 客户端身份 + 权限分档
    let name = name_of(&command);
    let client = match resolve_client(&config, &creds, &root, client_flag.as_deref(), name) {
        Ok(c) => c,
        Err(e) => return fail(json, e),
    };
    if let Err(e) = check_class(&client, class_of(&command), &config.harvest) {
        dex_store::audit::append(&root, &client.id, name, "-", e.code());
        return fail(json, e);
    }

    let ctx = Ctx {
        root,
        config,
        warnings,
        json,
        client,
        command: name,
    };
    let result = match &command {
        Commands::Search(args) => cmd::search::run(args, &ctx),
        Commands::Read(args) => cmd::read::run(args, &ctx),
        Commands::Propose(args) => cmd::propose::run(args, &ctx),
        Commands::Journal(args) => cmd::journal::run(args, &ctx),
        Commands::Render(args) => cmd::render::run(args, &ctx),
        Commands::Stale(args) => cmd::stale::run(args, &ctx),
        Commands::Review(args) => cmd::review::run(args, &ctx),
        Commands::Lint(args) => cmd::lint::run(args, &ctx),
        Commands::Reindex(args) => cmd::reindex_stub::run(args, &ctx),
        Commands::Harvest(args) => cmd::harvest::run(args, &ctx),
        Commands::Interview(args) => cmd::interview::run(args, &ctx),
        Commands::Init(_) | Commands::Skills(_) => unreachable!("已在前置分支处理"),
    };
    match result {
        Ok(code) => code,
        Err(e) => fail(json, e),
    }
}

/// pre-auth：TTY 物理在场即信任根直行；非交互须 config + 凭证 + human（FR-10.4）
fn run_pre_auth(
    json: bool,
    client_flag: Option<&str>,
    name: &'static str,
    f: impl FnOnce() -> Result<i32, DexError>,
) -> i32 {
    if guard_runtime::is_tty() {
        return match f() {
            Ok(code) => code,
            Err(e) => fail(json, e),
        };
    }
    let (config, _) = match config::load() {
        Ok(x) => x,
        Err(e) => return fail(json, e),
    };
    let root = config.root.clone();
    let (creds, _) = match config::load_credentials(&config.credentials_file) {
        Ok(x) => x,
        Err(e) => return fail(json, e),
    };
    let client = match resolve_client(&config, &creds, &root, client_flag, name) {
        Ok(c) => c,
        Err(e) => return fail(json, e),
    };
    if let Err(e) = check_class(&client, CommandClass::Management, &config.harvest) {
        dex_store::audit::append(&root, &client.id, name, "-", e.code());
        return fail(json, e);
    }
    match f() {
        Ok(code) => code,
        Err(e) => fail(json, e),
    }
}
