//! 配置加载（D1 契约，主会话实现）：两层 TOML 键级深合并（FR-10.2/DESIGN §2.5）＋
//! 本机凭据文件（FR-10.5）＋ W_CONFIG_CHANGED 基线（FR-10.7）。
//!
//! 加载序：
//! 1. 本机层 `~/.config/dex/config.toml`（XDG_CONFIG_HOME 优先；必选缺省——不存在即全缺省）
//! 2. root 解析：`DEX_ROOT` 环境变量 → 本机层 `root` → `~/dex`
//! 3. 仓库层 `<root>/.dex/config.toml`（可选）；含 `[auth]` ⇒ warning 并忽略（凭证永不进仓库层）
//! 4. 键级深合并：同叶键本机层覆盖仓库层（`[clients.<id>]`/`[harvest.sources.<id>]` 按 id 合并）
//! 5. 仓库层内容 SHA-256 与 `.cache/config.baseline` 比对 → W_CONFIG_CHANGED（无基线静默建立）
//! 6. human 内建缺省客户端（无需预注册，FR-10.3）；用户定义 `[clients.human]` 覆盖其字段

use dex_core::decay::StalePolicy;
use dex_core::inject::Budget;
use dex_core::{DexError, ScopeId, Warning};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------- 路径助手 ----------

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if p == "~" {
        home_dir()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(p)
    }
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("dex")
}

pub fn machine_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn default_credentials_path() -> PathBuf {
    config_dir().join("credentials.toml")
}

// ---------- 配置模型 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    Merged,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConf {
    pub interactive_default: bool,
    /// read = "full"（human 全量可读，FR-10.4）
    pub read_full: bool,
    pub scopes: Vec<ScopeId>,
    pub propose: bool,
    pub allowed_sources: Vec<String>,
    pub budget: Option<Budget>,
    pub proposals_per_day: usize,
    pub journal_per_day: usize,
    pub render_out: String,
    pub render_format: RenderFormat,
}

impl ClientConf {
    /// 非 human 客户端缺省（allowed_sources 缺省 = {客户端 id}，FR-4.7）
    fn default_for(id: &str) -> Self {
        ClientConf {
            interactive_default: false,
            read_full: false,
            scopes: vec![],
            propose: false,
            allowed_sources: vec![id.to_string()],
            budget: None,
            proposals_per_day: 20,
            journal_per_day: 60,
            render_out: "AGENTS.md".into(),
            render_format: RenderFormat::Merged,
        }
    }

    /// human 内建缺省（FR-10.3/10.4：TTY 免凭证、全量可读、source=human、可自提案）
    fn builtin_human() -> Self {
        ClientConf {
            interactive_default: true,
            read_full: true,
            propose: true,
            ..Self::default_for("human")
        }
    }
}

/// guard_runtime 用：内建 human 缺省（无需预注册，FR-10.3）
pub fn builtin_human_pub() -> ClientConf {
    ClientConf::builtin_human()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestSource {
    /// page | script | external（FR-12.6）
    pub source_type: String,
    pub first_batch: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestConf {
    pub budget_pulls: i64,
    pub budget_tokens: i64,
    pub budget_minutes: i64,
    pub sources: BTreeMap<String, HarvestSource>,
}

impl Default for HarvestConf {
    fn default() -> Self {
        // §2.5 缺省：pulls 50 / tokens 500000 / minutes 60
        HarvestConf {
            budget_pulls: 50,
            budget_tokens: 500_000,
            budget_minutes: 60,
            sources: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub root: PathBuf,
    /// 仅本机层 [auth] 解析结果（缺省 ~/.config/dex/credentials.toml）
    pub credentials_file: PathBuf,
    pub budget: Budget,
    /// clients 注册表（恒含内建 human）
    pub clients: BTreeMap<String, ClientConf>,
    pub stale: StalePolicy,
    pub git_auto_commit: bool,
    pub harvest: HarvestConf,
    /// 别名 → git remote 仓库名（FR-3.6）
    pub aliases: BTreeMap<String, String>,
    /// 密钥守卫 advisory 模式（FR-4.6「可配置」；§2.5 键表外新增键，已登记 debt）
    pub secret_advisory: bool,
}

// ---------- Raw 层（serde 宽松解析，字段全 Option） ----------

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawLayer {
    root: Option<String>,
    auth: Option<RawAuth>,
    budget: Option<RawBudget>,
    clients: BTreeMap<String, RawClient>,
    stale: Option<RawStale>,
    git: Option<RawGit>,
    harvest: Option<RawHarvest>,
    aliases: BTreeMap<String, RawAlias>,
    guard: Option<RawGuard>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAuth {
    credentials_file: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawBudget {
    entries: Option<usize>,
    chars: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawClient {
    interactive_default: Option<bool>,
    read: Option<String>,
    scopes: Option<Vec<String>>,
    propose: Option<bool>,
    allowed_sources: Option<Vec<String>>,
    budget: Option<RawBudget>,
    rate_limit: Option<RawRateLimit>,
    render: Option<RawRender>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawRateLimit {
    proposals_per_day: Option<usize>,
    journal_per_day: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawRender {
    out: Option<String>,
    format: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawStale {
    days: Option<i64>,
    scopes: Option<BTreeMap<String, RawStaleScope>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawStaleScope {
    days: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawGit {
    auto_commit: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawHarvest {
    budget: Option<RawHarvestBudget>,
    sources: Option<BTreeMap<String, RawHarvestSource>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawHarvestBudget {
    pulls: Option<i64>,
    tokens: Option<i64>,
    minutes: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawHarvestSource {
    #[serde(rename = "type")]
    source_type: Option<String>,
    first_batch: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAlias {
    remote: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawGuard {
    secret_advisory: Option<bool>,
}

// ---------- 加载 ----------

fn read_to_string_opt(path: &Path) -> Result<Option<String>, DexError> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(DexError::BadArgs {
            message: format!("读取 {} 失败：{e}", path.display()),
        }),
    }
}

fn parse_toml(text: &str, label: &str) -> Result<toml::Value, DexError> {
    text.parse::<toml::Value>().map_err(|e| DexError::BadArgs {
        message: format!("{label} TOML 解析失败：{e}"),
    })
}

/// 键级深合并：本机层覆盖仓库层（同叶键；表递归合并）
fn deep_merge(repo: &mut toml::Value, machine: &toml::Value) {
    match (repo, machine) {
        (toml::Value::Table(r), toml::Value::Table(m)) => {
            for (k, mv) in m {
                match r.entry(k.clone()) {
                    toml::map::Entry::Occupied(mut occ) => {
                        if occ.get().is_table() && mv.is_table() {
                            deep_merge(occ.get_mut(), mv);
                        } else {
                            occ.insert(mv.clone());
                        }
                    }
                    toml::map::Entry::Vacant(vac) => {
                        vac.insert(mv.clone());
                    }
                }
            }
        }
        (r, m) => *r = m.clone(),
    }
}

/// FR-10.7：仓库层内容 hash 与 .cache 基线比对（无基线静默建立——盲区已声明）
fn check_config_baseline(root: &Path, repo_text: &str, warnings: &mut Vec<Warning>) {
    let cache = root.join(".cache");
    let baseline = cache.join("config.baseline");
    let hash = format!("{:x}", Sha256::digest(repo_text.as_bytes()));
    match std::fs::read_to_string(&baseline) {
        Ok(old) if old.trim() == hash => {}
        Ok(_) => warnings.push(Warning::ConfigChanged),
        Err(_) => {}
    }
    let _ = std::fs::create_dir_all(&cache);
    let _ = std::fs::write(&baseline, &hash);
}

/// 加载两层配置。 Err = 配置非法（E_BAD_ARGS · 2）。
pub fn load() -> Result<(Config, Vec<Warning>), DexError> {
    let mut warnings = Vec::new();

    // 1. 本机层
    let machine_text = read_to_string_opt(&machine_config_path())?;
    let machine_val = machine_text
        .as_deref()
        .map(|t| parse_toml(t, "本机层 config"))
        .transpose()?;

    // 2. root 解析：DEX_ROOT → 本机层 root → ~/dex
    let env_root = std::env::var_os("DEX_ROOT").map(PathBuf::from);
    let machine_root = machine_val
        .as_ref()
        .and_then(|v| v.get("root"))
        .and_then(|v| v.as_str())
        .map(expand_tilde);
    let root = env_root
        .or(machine_root)
        .unwrap_or_else(|| home_dir().join("dex"));

    // [auth] 仅本机层
    let credentials_file = machine_val
        .as_ref()
        .and_then(|v| v.get("auth"))
        .and_then(|v| v.get("credentials_file"))
        .and_then(|v| v.as_str())
        .map(expand_tilde)
        .unwrap_or_else(default_credentials_path);

    // 3. 仓库层（[auth] 出现 ⇒ warning 并忽略，FR-10.2）
    let repo_path = root.join(".dex").join("config.toml");
    let repo_text = read_to_string_opt(&repo_path)?;
    let mut repo_val = repo_text
        .as_deref()
        .map(|t| parse_toml(t, "仓库层 config"))
        .transpose()?;
    if let Some(v) = repo_val.as_mut() {
        if let Some(table) = v.as_table_mut() {
            if table.remove("auth").is_some() {
                warnings.push(Warning::ConfigNotice {
                    detail: "[auth] 出现在仓库层 config——已忽略（凭证类键永不进仓库层，FR-10.2）"
                        .into(),
                });
            }
        }
    }

    // 5. W_CONFIG_CHANGED（仅当仓库层存在）
    if let Some(text) = &repo_text {
        check_config_baseline(&root, text, &mut warnings);
    }

    // 4. 深合并（本机层覆盖仓库层）
    let merged: toml::Value = match (repo_val, machine_val) {
        (Some(mut r), Some(m)) => {
            deep_merge(&mut r, &m);
            r
        }
        (Some(r), None) => r,
        (None, Some(m)) => m,
        (None, None) => toml::Value::Table(Default::default()),
    };

    let raw: RawLayer = merged.try_into().map_err(|e| DexError::BadArgs {
        message: format!("config 结构非法：{e}"),
    })?;

    let (config, build_warnings) = build_config(root, credentials_file, raw)?;
    warnings.extend(build_warnings);
    Ok((config, warnings))
}

fn build_config(
    root: PathBuf,
    credentials_file: PathBuf,
    raw: RawLayer,
) -> Result<(Config, Vec<Warning>), DexError> {
    let warnings = Vec::new();
    let bad = |m: String| DexError::BadArgs {
        message: format!("config 非法：{m}"),
    };

    let mut budget = Budget::default();
    if let Some(b) = &raw.budget {
        if let Some(e) = b.entries {
            budget.entries = e;
        }
        if let Some(c) = b.chars {
            budget.chars = c;
        }
    }

    let mut clients = BTreeMap::new();
    for (id, rc) in &raw.clients {
        let mut conf = if id == "human" {
            ClientConf::builtin_human()
        } else {
            ClientConf::default_for(id)
        };
        if let Some(v) = rc.interactive_default {
            conf.interactive_default = v;
        }
        if let Some(v) = &rc.read {
            conf.read_full = match v.as_str() {
                "full" => true,
                "scopes" => false,
                other => {
                    return Err(bad(format!(
                        "[clients.{id}].read 非法：{other:?}（full|scopes）"
                    )))
                }
            };
        }
        if let Some(list) = &rc.scopes {
            let mut scopes = Vec::new();
            for s in list {
                scopes
                    .push(ScopeId::new(s).map_err(|e| bad(format!("[clients.{id}].scopes：{e}")))?);
            }
            conf.scopes = scopes;
        }
        if let Some(v) = rc.propose {
            conf.propose = v;
        }
        if let Some(v) = &rc.allowed_sources {
            conf.allowed_sources = v.clone();
        }
        if let Some(b) = &rc.budget {
            let mut cb = conf.budget.unwrap_or(budget);
            if let Some(e) = b.entries {
                cb.entries = e;
            }
            if let Some(c) = b.chars {
                cb.chars = c;
            }
            conf.budget = Some(cb);
        }
        if let Some(rl) = &rc.rate_limit {
            if let Some(v) = rl.proposals_per_day {
                conf.proposals_per_day = v;
            }
            if let Some(v) = rl.journal_per_day {
                conf.journal_per_day = v;
            }
        }
        if let Some(r) = &rc.render {
            if let Some(v) = &r.out {
                conf.render_out = v.clone();
            }
            if let Some(v) = &r.format {
                conf.render_format = match v.as_str() {
                    "merged" => RenderFormat::Merged,
                    "import" => RenderFormat::Import,
                    other => {
                        return Err(bad(format!(
                            "[clients.{id}].render.format 非法：{other:?}（merged|import）"
                        )))
                    }
                };
            }
        }
        clients.insert(id.clone(), conf);
    }
    // human 内建缺省：无需预注册（FR-10.3）
    clients
        .entry("human".to_string())
        .or_insert_with(ClientConf::builtin_human);

    let mut stale = StalePolicy {
        default_days: 90,
        per_scope: vec![],
    };
    if let Some(s) = &raw.stale {
        if let Some(d) = s.days {
            stale.default_days = d;
        }
        if let Some(map) = &s.scopes {
            for (prefix, ps) in map {
                let days = ps
                    .days
                    .ok_or_else(|| bad(format!("[stale.scopes.{prefix:?}] 缺 days")))?;
                stale.per_scope.push((prefix.clone(), days));
            }
        }
    }

    let git_auto_commit = raw.git.as_ref().and_then(|g| g.auto_commit).unwrap_or(true);

    let mut harvest = HarvestConf::default();
    if let Some(h) = &raw.harvest {
        if let Some(b) = &h.budget {
            if let Some(v) = b.pulls {
                harvest.budget_pulls = v;
            }
            if let Some(v) = b.tokens {
                harvest.budget_tokens = v;
            }
            if let Some(v) = b.minutes {
                harvest.budget_minutes = v;
            }
        }
        if let Some(map) = &h.sources {
            for (id, s) in map {
                let source_type = s.source_type.clone().unwrap_or_else(|| "page".into());
                if !matches!(source_type.as_str(), "page" | "script" | "external") {
                    return Err(bad(format!(
                        "[harvest.sources.{id}].type 非法：{source_type:?}（page|script|external）"
                    )));
                }
                harvest.sources.insert(
                    id.clone(),
                    HarvestSource {
                        source_type,
                        first_batch: s.first_batch.unwrap_or(30),
                    },
                );
            }
        }
    }

    let aliases = raw
        .aliases
        .iter()
        .filter_map(|(k, v)| v.remote.clone().map(|r| (k.clone(), r)))
        .collect();

    let secret_advisory = raw
        .guard
        .as_ref()
        .and_then(|g| g.secret_advisory)
        .unwrap_or(false);

    Ok((
        Config {
            root,
            credentials_file,
            budget,
            clients,
            stale,
            git_auto_commit,
            harvest,
            aliases,
            secret_advisory,
        },
        warnings,
    ))
}

// ---------- 凭据文件（FR-10.5 / SECURITY §5） ----------

#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub tokens: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawCredentials {
    clients: BTreeMap<String, RawToken>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawToken {
    token: Option<String>,
}

/// 加载本机凭据文件。不存在 → 空（= 该客户端无凭证）；解析失败 → E_BAD_ARGS；
/// 权限宽于 0600 → W_CREDS_PERMS（不阻断，FR-10.5）。token 长度校验（≥32）在使用时执行。
pub fn load_credentials(path: &Path) -> Result<(Credentials, Vec<Warning>), DexError> {
    let mut warnings = Vec::new();
    let text = match read_to_string_opt(path)? {
        None => return Ok((Credentials::default(), warnings)),
        Some(t) => t,
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o077 != 0 {
                warnings.push(Warning::CredsPerms {
                    path: path.display().to_string(),
                });
            }
        }
    }
    let raw: RawCredentials = text
        .parse::<toml::Value>()
        .map_err(|e| DexError::BadArgs {
            message: format!("凭据文件 {} 解析失败：{e}", path.display()),
        })?
        .try_into()
        .map_err(|e| DexError::BadArgs {
            message: format!("凭据文件结构非法：{e}"),
        })?;
    let tokens = raw
        .clients
        .into_iter()
        .filter_map(|(id, t)| t.token.filter(|s| !s.is_empty()).map(|v| (id, v)))
        .collect();
    Ok((Credentials { tokens }, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_merge_leaf_and_table() {
        let mut repo = r#"
[budget]
entries = 10
chars = 2000
[clients."choose-you"]
scopes = ["person"]
propose = false
"#
        .parse::<toml::Value>()
        .unwrap();
        let machine = r#"
[clients."choose-you"]
propose = true
[stale]
days = 120
"#
        .parse::<toml::Value>()
        .unwrap();
        deep_merge(&mut repo, &machine);
        assert_eq!(repo["budget"]["entries"].as_integer(), Some(10));
        assert_eq!(
            repo["clients"]["choose-you"]["propose"].as_bool(),
            Some(true)
        );
        assert_eq!(
            repo["clients"]["choose-you"]["scopes"]
                .as_array()
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(repo["stale"]["days"].as_integer(), Some(120));
    }

    #[test]
    fn human_builtin_defaults() {
        let conf = ClientConf::builtin_human();
        assert!(conf.read_full && conf.interactive_default && conf.propose);
        assert_eq!(conf.allowed_sources, vec!["human".to_string()]);
    }
}
