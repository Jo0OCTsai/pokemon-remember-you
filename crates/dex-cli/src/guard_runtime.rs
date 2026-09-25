//! 接入守卫运行时（D1 契约，主会话实现）：客户端身份解析（FR-10.3 TTY human 免凭证 /
//! 非交互 --client + 凭证）、管理命令权限分档（FR-10.4）、source 绑定（FR-4.7 守卫步骤 0）。
//!
//! TTY 判定（冻结）：stdin ∧ stdout 均为终端 = 交互式（物理在场即信任根，SECURITY §2 边界⑤）。
//! token 解析：`DEX_TOKEN` 环境变量覆盖 → 本机凭据文件；长度 <32 字符视为无凭证（FR-10.5）。

use crate::config::{ClientConf, Config, Credentials, HarvestConf};
use crate::output;
use dex_core::guard::GuardLimits;
use dex_core::{DexError, ReadGrant, Warning};
use dex_store::audit;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// token 最小长度（≥256-bit 熵的代理指标，FR-10.5）
pub const MIN_TOKEN_CHARS: usize = 32;

#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub conf: ClientConf,
    /// 解析时是否 TTY（信息保留：v2 MCP 会话/审计呈现可用；v1 命令面不消费）
    #[allow(dead_code)]
    pub tty: bool,
}

/// 命令权限分档（FR-10.4）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandClass {
    /// search / read
    Read,
    /// propose / journal（须 [clients].propose = true）
    Write,
    /// render/review/stale/lint/reindex/init/skills——仅 human
    Management,
    /// harvest / interview——human 或显式授权的收割客户端（harvest-<source> ↔ [harvest.sources]）
    Harvest,
}

/// 命令执行上下文（main 装配，命令消费）
pub struct Ctx {
    pub root: PathBuf,
    pub config: Config,
    /// 加载期警告（W_CREDS_PERMS / W_CONFIG_CHANGED / W_CONFIG_NOTICE）——并入每个成功信封
    pub warnings: Vec<Warning>,
    pub json: bool,
    pub client: Client,
    pub command: &'static str,
}

impl Ctx {
    /// 调用客户端读授权
    pub fn grant(&self) -> ReadGrant {
        if self.client.conf.read_full {
            ReadGrant::Full
        } else {
            ReadGrant::Scopes(self.client.conf.scopes.clone())
        }
    }

    /// 守卫限流参数（含 secret advisory 配置；bootstrap 模式限流放宽）
    pub fn limits(&self, bootstrap_first_batch: Option<usize>) -> GuardLimits {
        GuardLimits {
            proposals_per_day: self.client.conf.proposals_per_day,
            journal_per_day: self.client.conf.journal_per_day,
            secret_advisory: self.config.secret_advisory,
            bootstrap_first_batch,
        }
    }

    /// 追加协议面拒绝事件（尽力而为，FR-10.6）
    pub fn audit(&self, requested: &str, code: &str) {
        audit::append(&self.root, &self.client.id, self.command, requested, code);
    }

    /// 统一成功输出：json 信封（含加载期警告 + extra）或 text；返回退出码
    pub fn emit(
        &self,
        code: i32,
        extra: Vec<Warning>,
        data: serde_json::Value,
        text: String,
    ) -> i32 {
        if self.json {
            let mut ws = self.warnings.clone();
            ws.extend(extra);
            output::print_ok(&ws, data);
        } else {
            if !text.is_empty() {
                print!("{text}");
            }
            for w in extra {
                eprintln!("⚠ {} {}", w.code(), w.message());
            }
        }
        code
    }

    /// 注入预算：per-client 覆盖 → 全局 [budget]
    pub fn budget_for(&self, client_id: &str) -> dex_core::inject::Budget {
        self.config
            .clients
            .get(client_id)
            .and_then(|c| c.budget)
            .unwrap_or(self.config.budget)
    }
}

pub fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn token_valid(t: &str) -> bool {
    t.chars().count() >= MIN_TOKEN_CHARS
}

/// 解析客户端身份（FR-10.3）。拒绝事件直接写 audit（root 存在时尽力而为）。
pub fn resolve_client(
    config: &Config,
    creds: &Credentials,
    root: &Path,
    client_flag: Option<&str>,
    command: &'static str,
) -> Result<Client, DexError> {
    let tty = is_tty();
    match client_flag {
        Some(id) => {
            let Some(conf) = config.clients.get(id) else {
                audit::append(root, "unknown", command, "-", "E_SCOPE_DENIED");
                return Err(DexError::ScopeDenied {
                    detail: format!("未注册客户端 {id:?}（未注册 = 全拒，FR-10.3）"),
                });
            };
            // human TTY 免凭证（内建信任根）
            if id == "human" && tty {
                return Ok(Client {
                    id: id.to_string(),
                    conf: conf.clone(),
                    tty,
                });
            }
            let token = std::env::var("DEX_TOKEN")
                .ok()
                .filter(|t| !t.is_empty())
                .or_else(|| creds.tokens.get(id).cloned());
            match token {
                Some(t) if token_valid(&t) => Ok(Client {
                    id: id.to_string(),
                    conf: conf.clone(),
                    tty,
                }),
                _ => {
                    audit::append(root, id, command, "-", "E_SCOPE_DENIED");
                    Err(DexError::ScopeDenied {
                        detail: format!("客户端 {id:?} 无有效凭证（token 缺失或 <{MIN_TOKEN_CHARS} 字符，FR-10.5）"),
                    })
                }
            }
        }
        None => {
            if tty {
                let conf = config
                    .clients
                    .get("human")
                    .cloned()
                    .unwrap_or_else(crate::config::builtin_human_pub);
                Ok(Client {
                    id: "human".to_string(),
                    conf,
                    tty,
                })
            } else {
                audit::append(root, "unknown", command, "-", "E_SCOPE_DENIED");
                Err(DexError::ScopeDenied {
                    detail: "非交互调用须显式 --client <id>（FR-10.3）".into(),
                })
            }
        }
    }
}

/// 管理分档校验（FR-10.4）
pub fn check_class(
    client: &Client,
    class: CommandClass,
    harvest: &HarvestConf,
) -> Result<(), DexError> {
    match class {
        CommandClass::Management if client.id != "human" => Err(DexError::ScopeDenied {
            detail: format!("管理命令仅 human 可执行（当前 {:?}，FR-10.4）", client.id),
        }),
        CommandClass::Harvest => {
            let ok = client.id == "human"
                || client
                    .id
                    .strip_prefix("harvest-")
                    .is_some_and(|src| harvest.sources.contains_key(src));
            if ok {
                Ok(())
            } else {
                Err(DexError::ScopeDenied {
                    detail: format!(
                        "harvest/interview 仅 human 或已注册收割客户端可执行（当前 {:?}，FR-12.5）",
                        client.id
                    ),
                })
            }
        }
        CommandClass::Write if !client.conf.propose => Err(DexError::ScopeDenied {
            detail: format!(
                "客户端 {:?} 无提案权（[clients].propose，FR-10.1）",
                client.id
            ),
        }),
        _ => Ok(()),
    }
}

/// 守卫步骤 0：source 与客户端身份绑定（FR-4.7）——调用方负责 audit
pub fn check_source_binding(client: &Client, source: &str) -> Result<(), DexError> {
    if client.conf.allowed_sources.iter().any(|s| s == source) {
        Ok(())
    } else {
        Err(DexError::SourceMismatch {
            source: source.to_string(),
            client: client.id.clone(),
        })
    }
}

/// 今日本地日期 `YYYY-MM-DD`
pub fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
