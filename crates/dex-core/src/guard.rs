//! §5.5 提案校验序（守卫核心）。步骤 0（客户端身份与 source 绑定）在 cli 的
//! guard_runtime；**步骤 1–8 为纯函数**，校验序与错误码严格如下（全部通过才落盘）：
//!
//! ```text
//!  1 source 非空 ∧ 匹配 ^[a-z0-9][a-z0-9-]{0,31}$        → E_BAD_SOURCE · 2
//!  2 evidence 非空 ∧ ≤2000（Unicode）                    → 空: E_NO_EVIDENCE · 4 / 超长: E_TOO_LARGE · 5
//!  3 kind ∈ {fact, preference, pattern}                  → E_BAD_ARGS · 2
//!  4 confidence ∈ [0,100]（缺省 50）                      → E_BAD_ARGS · 2
//!  5 正文 ≤ 4000（Unicode）                               → E_TOO_LARGE · 5
//!  6 限流：count(inbox 今日该 source) < proposals_per_day  → E_RATE_LIMIT · 5
//!        bootstrap 模式：限流上限放宽为 first_batch（缺省 30，FR-4.3）
//!  7 幂等：SHA-256(source+kind+content) 对未裁决提案命中   → 返回既有文件（幂等成功；
//!        confidence/evidence 差异不破坏幂等）
//!  8 密钥守卫：gitleaks 类正则命中 ⇒ 默认拒绝（E_SECRET · 9；advisory 仅警告放行）
//! 通过 → 计算 shortid（同日同 source 文件名前缀碰撞顺延；耗尽 → E_BAD_ARGS）
//! ```
//! journal 供稿走**子集 0/1/5/6/8**（限流用 `journal_per_day` 缺省 60，§5.5 末行）。
//!
//! **B3 任务（TDD）**：先写失败测试（每条失败路径至少一例）再实现；公共 API 冻结。

use crate::errors::DexError;
use crate::proposal;
use regex::Regex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

/// 守卫限流参数（§5.5-6、§2.5 键表）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardLimits {
    /// proposals_per_day（缺省 20，FR-4.3）
    pub proposals_per_day: usize,
    /// journal_per_day（缺省 60，FR-5.4）
    pub journal_per_day: usize,
    /// 密钥守卫 advisory 模式（默认 false = 命中即拒，FR-4.6）
    pub secret_advisory: bool,
    /// bootstrap 模式限流上限（FR-4.3 首批 ≤30；None = 非 bootstrap）
    pub bootstrap_first_batch: Option<usize>,
}

impl Default for GuardLimits {
    fn default() -> Self {
        GuardLimits {
            proposals_per_day: 20,
            journal_per_day: 60,
            secret_advisory: false,
            bootstrap_first_batch: None,
        }
    }
}

/// 提案输入
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalInput {
    pub source: String,
    pub kind: String,
    pub confidence: Option<i64>,
    pub evidence: String,
    pub content: String,
}

/// 未裁决提案（inbox 顶层文件解析产物；幂等比对域 = source+kind+content）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingProposal {
    pub path: PathBuf,
    pub source: String,
    pub kind: String,
    pub content: String,
}

/// inbox 视图端口（DESIGN §9 注：core 声明端口、store 实现——core 不做 I/O）
pub trait InboxView {
    /// 今日该 source 已落盘提案数（按 inbox **顶层**文件名日期前缀；staging 不计，§5.5-6）
    fn count_today(&self, source: &str, date: &str) -> Result<usize, DexError>;
    /// 今日该 source 的 inbox 顶层文件名全集（shortid 碰撞顺延判定用）
    fn today_filenames(&self, source: &str, date: &str) -> Result<Vec<String>, DexError>;
    /// 未裁决提案全集（frontmatter 解析后；幂等查重用）
    fn pending(&self) -> Result<Vec<PendingProposal>, DexError>;
}

/// 守卫通过产物
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardOutcome {
    /// 落盘文件名（仅文件名；inbox/ 前缀由调用方拼）
    pub filename: String,
    /// 幂等 hash（§5.5-7）
    pub hash: String,
    /// 采纳的 shortid
    pub shortid: String,
    /// 幂等命中（重提交返回既有文件路径，FR-4.1）
    pub idempotent_existing: Option<PathBuf>,
}

/// 提案守卫（§5.5 步骤 1–8 + shortid；步骤 0 由 guard_runtime 先行完成）。
/// 校验序即错误优先级：1 source → 2 evidence → 3 kind → 4 confidence → 5 正文 →
/// 6 限流（bootstrap_first_batch 放宽上限）→ 7 幂等（命中即成功返回，跳过步骤 8）→
/// 8 密钥（advisory 放行）→ pick_shortid → filename。
pub fn check_proposal(
    input: &ProposalInput,
    limits: &GuardLimits,
    inbox: &dyn InboxView,
    date: &str,
) -> Result<GuardOutcome, DexError> {
    // 1 source 格式
    proposal::validate_source_format(&input.source)?;
    // 2 evidence 非空 ∧ ≤2000（Unicode 字符数）
    if input.evidence.trim().is_empty() {
        return Err(DexError::NoEvidence);
    }
    let ev_len = input.evidence.chars().count();
    if ev_len > proposal::EVIDENCE_MAX_CHARS {
        return Err(DexError::TooLarge {
            what: "evidence",
            limit: proposal::EVIDENCE_MAX_CHARS,
            actual: ev_len,
        });
    }
    // 3 kind 枚举
    if !proposal::KINDS.contains(&input.kind.as_str()) {
        return Err(DexError::BadArgs {
            message: format!(
                "kind 非枚举：{:?}（须为 fact|preference|pattern）",
                input.kind
            ),
        });
    }
    // 4 confidence ∈ [0,100]（缺省 50）
    let confidence = input.confidence.unwrap_or(proposal::DEFAULT_CONFIDENCE);
    if !(0..=100).contains(&confidence) {
        return Err(DexError::BadArgs {
            message: format!("confidence 越界：{confidence}（须 0–100）"),
        });
    }
    // 5 正文 ≤ 4000（Unicode 字符数）
    let content_len = input.content.chars().count();
    if content_len > proposal::CONTENT_MAX_CHARS {
        return Err(DexError::TooLarge {
            what: "正文",
            limit: proposal::CONTENT_MAX_CHARS,
            actual: content_len,
        });
    }
    // 6 限流：今日已落盘 ≥ 上限 → 拒（bootstrap 模式上限放宽为 first_batch）
    let limit = limits
        .bootstrap_first_batch
        .unwrap_or(limits.proposals_per_day);
    if inbox.count_today(&input.source, date)? >= limit {
        return Err(DexError::RateLimit {
            source: input.source.clone(),
            limit,
        });
    }
    // 7 幂等：比对域 = source+kind+content（confidence/evidence 不入域）；命中即成功返回（跳过步骤 8）
    let hash = proposal::proposal_hash(&input.source, &input.kind, &input.content);
    for p in inbox.pending()? {
        if proposal::proposal_hash(&p.source, &p.kind, &p.content) == hash {
            let filename = p
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let shortid = shortid_of_filename(&filename);
            return Ok(GuardOutcome {
                filename,
                hash,
                shortid,
                idempotent_existing: Some(p.path.clone()),
            });
        }
    }
    // 8 密钥守卫：扫描对象 = 正文 + evidence 拼接；命中且非 advisory → 拒（首个命中模式）
    let hits = scan_secrets(&format!("{}{}", input.content, input.evidence));
    if !hits.is_empty() && !limits.secret_advisory {
        return Err(DexError::Secret {
            pattern: hits[0].to_string(),
        });
    }
    // 通过 → shortid 碰撞顺延 + 文件名
    let existing = inbox.today_filenames(&input.source, date)?;
    let shortid = pick_shortid(&hash, &existing)?;
    let filename = proposal::proposal_filename(date, &input.source, &shortid);
    Ok(GuardOutcome {
        filename,
        hash,
        shortid,
        idempotent_existing: None,
    })
}

/// journal 供稿守卫子集（步骤 1/5/6/8；步骤 0 由 guard_runtime 先行完成）。
/// `today_count` = 今日该 source 已供稿条数（journal 页小节内列表项计数，由调用方统计）。
pub fn check_journal(
    source: &str,
    text: &str,
    limits: &GuardLimits,
    today_count: usize,
) -> Result<(), DexError> {
    // 1 source 格式
    proposal::validate_source_format(source)?;
    // 5 text ≤ 4000
    let len = text.chars().count();
    if len > proposal::CONTENT_MAX_CHARS {
        return Err(DexError::TooLarge {
            what: "正文",
            limit: proposal::CONTENT_MAX_CHARS,
            actual: len,
        });
    }
    // 6 日限流（journal_per_day）
    if today_count >= limits.journal_per_day {
        return Err(DexError::RateLimit {
            source: source.to_string(),
            limit: limits.journal_per_day,
        });
    }
    // 8 密钥守卫（同款模式；advisory 放行）
    let hits = scan_secrets(text);
    if !hits.is_empty() && !limits.secret_advisory {
        return Err(DexError::Secret {
            pattern: hits[0].to_string(),
        });
    }
    Ok(())
}

/// 密钥模式表（gitleaks 类正则，§5.5-8）：（模式名, 正则），表序即命中返回序。
const SECRET_PATTERNS: &[(&str, &str)] = &[
    ("aws-access-key", r"AKIA[0-9A-Z]{16}"),
    ("github-token", r"gh[pousr]_[A-Za-z0-9]{36}"),
    ("slack-token", r"xox[baprs]-[0-9A-Za-z-]{10,}"),
    ("private-key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ("google-api-key", r"AIza[0-9A-Za-z_-]{35}"),
    (
        "generic-api-key",
        r#"(?i)(api[_-]?key|secret|token)\s*[:=]\s*["']?[A-Za-z0-9_\-/.+]{20,}"#,
    ),
];

fn secret_regexes() -> &'static Vec<(&'static str, Regex)> {
    static COMPILED: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        SECRET_PATTERNS
            .iter()
            .map(|(name, re)| {
                (
                    *name,
                    Regex::new(re).expect("密钥模式正则为冻结常量，必合法"),
                )
            })
            .collect()
    })
}

/// 密钥模式扫描（gitleaks 类正则，§5.5-8；扫描对象 = 正文 + evidence）。
/// 返回命中的模式名列表（空 = 干净；按模式表序去重返回）。
pub fn scan_secrets(text: &str) -> Vec<&'static str> {
    secret_regexes()
        .iter()
        .filter(|(_, re)| re.is_match(text))
        .map(|(name, _)| *name)
        .collect()
}

/// 计算 shortid：对 `hash` 依次取 [`proposal::shortid_candidates`] 段，
/// 与 `existing_today_filenames`（同日同 source）无前缀冲突的第一段；耗尽 → E_BAD_ARGS。
/// 冲突判定：既有文件名（`<date>-<source>-<shortid>.md`，同日同 source 前提下）的
/// shortid 段（最后一个 `-` 之后）与候选段相等。
pub fn pick_shortid(hash: &str, existing_today_filenames: &[String]) -> Result<String, DexError> {
    let taken: HashSet<&str> = existing_today_filenames
        .iter()
        .filter_map(|name| name.strip_suffix(".md"))
        .filter_map(|stem| stem.rsplit_once('-'))
        .map(|(_, shortid)| shortid)
        .filter(|shortid| !shortid.is_empty())
        .collect();
    for candidate in proposal::shortid_candidates(hash) {
        if !taken.contains(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err(DexError::BadArgs {
        message: "shortid 候选段耗尽（同日同 source 文件名前缀持续碰撞，§2.3）".to_string(),
    })
}

/// 从既有文件名提取 shortid 段（幂等命中时回填 GuardOutcome 用）。
fn shortid_of_filename(filename: &str) -> String {
    filename
        .strip_suffix(".md")
        .and_then(|stem| stem.rsplit_once('-'))
        .map(|(_, shortid)| shortid.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_err(err: &DexError, code: &str, exit: i32) {
        assert_eq!(err.code(), code);
        assert_eq!(err.exit_code(), exit);
    }

    fn input(
        source: &str,
        kind: &str,
        confidence: Option<i64>,
        evidence: &str,
        content: &str,
    ) -> ProposalInput {
        ProposalInput {
            source: source.to_string(),
            kind: kind.to_string(),
            confidence,
            evidence: evidence.to_string(),
            content: content.to_string(),
        }
    }

    fn ok_input() -> ProposalInput {
        input("choose-you", "fact", Some(80), "chat_feedback #1", "内容")
    }

    struct MockInbox {
        count: usize,
        filenames: Vec<String>,
        pending: Vec<PendingProposal>,
    }

    fn empty_inbox() -> MockInbox {
        MockInbox {
            count: 0,
            filenames: Vec::new(),
            pending: Vec::new(),
        }
    }

    impl InboxView for MockInbox {
        fn count_today(&self, _source: &str, _date: &str) -> Result<usize, DexError> {
            Ok(self.count)
        }
        fn today_filenames(&self, _source: &str, _date: &str) -> Result<Vec<String>, DexError> {
            Ok(self.filenames.clone())
        }
        fn pending(&self) -> Result<Vec<PendingProposal>, DexError> {
            Ok(self.pending.clone())
        }
    }

    // ---------- check_proposal：通过路径 ----------

    #[test]
    fn proposal_happy_path() {
        let out = check_proposal(
            &ok_input(),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap();
        let h = proposal::proposal_hash("choose-you", "fact", "内容");
        assert_eq!(out.hash, h);
        assert_eq!(out.shortid, h[0..6]);
        assert_eq!(
            out.filename,
            proposal::proposal_filename("2026-09-25", "choose-you", &h[0..6])
        );
        assert_eq!(out.idempotent_existing, None);
    }

    // ---------- check_proposal：每条失败路径 ----------

    #[test]
    fn proposal_step1_bad_source() {
        let err = check_proposal(
            &input("Bad_Source", "fact", None, "ev", "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_BAD_SOURCE", 2);
    }

    #[test]
    fn proposal_step2_evidence_empty_and_too_large() {
        let err = check_proposal(
            &input("s", "fact", None, "  ", "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_NO_EVIDENCE", 4);
        let err = check_proposal(
            &input("s", "fact", None, &"字".repeat(2001), "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_eq!(
            err,
            DexError::TooLarge {
                what: "evidence",
                limit: 2000,
                actual: 2001
            }
        );
        assert_err(&err, "E_TOO_LARGE", 5);
        // 边界：恰 2000 通过
        let out = check_proposal(
            &input("s", "fact", None, &"字".repeat(2000), "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap();
        assert_eq!(out.idempotent_existing, None);
    }

    #[test]
    fn proposal_step3_step4_bad_kind_and_confidence() {
        // 步骤 3：kind 非枚举
        let err = check_proposal(
            &input("s", "diary", None, "e", "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_BAD_ARGS", 2);
        // 步骤 4：confidence 越界（>100 / <0）
        for bad_conf in [Some(101i64), Some(-1i64)] {
            let err = check_proposal(
                &input("s", "fact", bad_conf, "e", "c"),
                &GuardLimits::default(),
                &empty_inbox(),
                "2026-09-25",
            )
            .unwrap_err();
            assert_err(&err, "E_BAD_ARGS", 2);
        }
        // 边界：0 / 100 / 缺省（→50）均通过
        for ok_conf in [Some(0i64), Some(100i64), None] {
            assert!(check_proposal(
                &input("s", "fact", ok_conf, "e", "c"),
                &GuardLimits::default(),
                &empty_inbox(),
                "2026-09-25"
            )
            .is_ok());
        }
    }

    #[test]
    fn proposal_step5_content_too_large() {
        let err = check_proposal(
            &input("s", "fact", None, "e", &"字".repeat(4001)),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_eq!(
            err,
            DexError::TooLarge {
                what: "正文",
                limit: 4000,
                actual: 4001
            }
        );
        assert_err(&err, "E_TOO_LARGE", 5);
        // 边界：恰 4000 通过
        assert!(check_proposal(
            &input("s", "fact", None, "e", &"字".repeat(4000)),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25"
        )
        .is_ok());
    }

    #[test]
    fn proposal_step6_rate_limit_boundaries() {
        let limits = GuardLimits::default();
        // 19 过 / 20 拒（缺省 proposals_per_day = 20）
        let inbox19 = MockInbox {
            count: 19,
            ..empty_inbox()
        };
        assert!(check_proposal(&ok_input(), &limits, &inbox19, "2026-09-25").is_ok());
        let inbox20 = MockInbox {
            count: 20,
            ..empty_inbox()
        };
        let err = check_proposal(&ok_input(), &limits, &inbox20, "2026-09-25").unwrap_err();
        assert_eq!(
            err,
            DexError::RateLimit {
                source: "choose-you".to_string(),
                limit: 20
            }
        );
        assert_err(&err, "E_RATE_LIMIT", 5);
        // bootstrap 放宽 30：29 过 / 30 拒
        let boot = GuardLimits {
            bootstrap_first_batch: Some(30),
            ..GuardLimits::default()
        };
        let inbox29 = MockInbox {
            count: 29,
            ..empty_inbox()
        };
        assert!(check_proposal(&ok_input(), &boot, &inbox29, "2026-09-25").is_ok());
        let inbox30 = MockInbox {
            count: 30,
            ..empty_inbox()
        };
        let err = check_proposal(&ok_input(), &boot, &inbox30, "2026-09-25").unwrap_err();
        assert_err(&err, "E_RATE_LIMIT", 5);
    }

    // ---------- check_proposal：幂等（步骤 7） ----------

    #[test]
    fn proposal_step7_idempotent_hit() {
        let path = PathBuf::from("inbox/2026-09-24-choose-you-deadbe.md");
        let inbox = MockInbox {
            pending: vec![PendingProposal {
                path: path.clone(),
                source: "choose-you".to_string(),
                kind: "fact".to_string(),
                content: "内容".to_string(),
            }],
            ..empty_inbox()
        };
        // 同 source+kind+content 重提交；confidence/evidence 不同不破坏幂等
        let i = input("choose-you", "fact", Some(1), "另一个证据", "内容");
        let out = check_proposal(&i, &GuardLimits::default(), &inbox, "2026-09-25").unwrap();
        assert_eq!(out.idempotent_existing, Some(path));
        assert_eq!(out.filename, "2026-09-24-choose-you-deadbe.md");
        assert_eq!(out.shortid, "deadbe");
        assert_eq!(
            out.hash,
            proposal::proposal_hash("choose-you", "fact", "内容")
        );
        // 内容不同 → 不命中，正常路径
        let out2 = check_proposal(
            &input(
                "choose-you",
                "fact",
                Some(80),
                "chat_feedback #1",
                "不同内容",
            ),
            &GuardLimits::default(),
            &inbox,
            "2026-09-25",
        )
        .unwrap();
        assert_eq!(out2.idempotent_existing, None);
    }

    #[test]
    fn proposal_idempotent_skips_secret_scan() {
        // §7.3：幂等命中即成功返回，跳过密钥扫描（步骤 8）
        let secret = "AKIAIOSFODNN7EXAMPLE";
        let inbox = MockInbox {
            pending: vec![PendingProposal {
                path: PathBuf::from("inbox/2026-09-24-s-ffeedd.md"),
                source: "s".to_string(),
                kind: "fact".to_string(),
                content: secret.to_string(),
            }],
            ..empty_inbox()
        };
        let i = input("s", "fact", None, "ev", secret);
        let out = check_proposal(&i, &GuardLimits::default(), &inbox, "2026-09-25").unwrap();
        assert_eq!(
            out.idempotent_existing,
            Some(PathBuf::from("inbox/2026-09-24-s-ffeedd.md"))
        );
        // 对照：无既有提案时同内容密钥 → 拒
        let err =
            check_proposal(&i, &GuardLimits::default(), &empty_inbox(), "2026-09-25").unwrap_err();
        assert_err(&err, "E_SECRET", 9);
    }

    // ---------- check_proposal：密钥（步骤 8） ----------

    #[test]
    fn proposal_step8_secret_reject_and_advisory() {
        let err = check_proposal(
            &input("s", "fact", None, "ev", "AKIAIOSFODNN7EXAMPLE"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_eq!(
            err,
            DexError::Secret {
                pattern: "aws-access-key".to_string()
            }
        );
        assert_err(&err, "E_SECRET", 9);
        // evidence 侧密钥同样拦截（扫描对象 = 正文 + evidence）
        let err = check_proposal(
            &input("s", "fact", None, "AKIAIOSFODNN7EXAMPLE", "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_SECRET", 9);
        // advisory 模式放行（调用方从 warnings 感知）
        let adv = GuardLimits {
            secret_advisory: true,
            ..GuardLimits::default()
        };
        assert!(check_proposal(
            &input("s", "fact", None, "ev", "AKIAIOSFODNN7EXAMPLE"),
            &adv,
            &empty_inbox(),
            "2026-09-25"
        )
        .is_ok());
    }

    // ---------- 校验序优先级 ----------

    #[test]
    fn proposal_order_priority() {
        // 1 在 2 前：source 非法 + evidence 空 → E_BAD_SOURCE
        let err = check_proposal(
            &input("BAD", "fact", None, "", "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_BAD_SOURCE", 2);
        // 2 在 3 前：evidence 空 + kind 非枚举 → E_NO_EVIDENCE
        let err = check_proposal(
            &input("s", "diary", None, "", "c"),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_NO_EVIDENCE", 4);
        // 4 在 5 前：confidence 越界 + 正文超长 → E_BAD_ARGS
        let err = check_proposal(
            &input("s", "fact", Some(101), "e", &"x".repeat(4001)),
            &GuardLimits::default(),
            &empty_inbox(),
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_BAD_ARGS", 2);
        // 6 在 7 前：限流已满时即使幂等命中也先拒（严格校验序）
        let inbox = MockInbox {
            count: 20,
            pending: vec![PendingProposal {
                path: PathBuf::from("inbox/2026-09-25-choose-you-abc123.md"),
                source: "choose-you".to_string(),
                kind: "fact".to_string(),
                content: "内容".to_string(),
            }],
            ..empty_inbox()
        };
        let err =
            check_proposal(&ok_input(), &GuardLimits::default(), &inbox, "2026-09-25").unwrap_err();
        assert_err(&err, "E_RATE_LIMIT", 5);
        // 6 在 8 前：限流 + 密钥 → E_RATE_LIMIT
        let err = check_proposal(
            &input("s", "fact", None, "e", "AKIAIOSFODNN7EXAMPLE"),
            &GuardLimits::default(),
            &MockInbox {
                count: 20,
                ..empty_inbox()
            },
            "2026-09-25",
        )
        .unwrap_err();
        assert_err(&err, "E_RATE_LIMIT", 5);
    }

    // ---------- pick_shortid ----------

    #[test]
    fn shortid_first_free_segment() {
        let h = proposal::proposal_hash("s", "fact", "c");
        // 无冲突 → 首段
        assert_eq!(pick_shortid(&h, &[]).unwrap(), h[0..6]);
        // 同日同 source 已占首段 → 顺延 [6..12]
        let f1 = proposal::proposal_filename("2026-09-25", "s", &h[0..6]);
        assert_eq!(
            pick_shortid(&h, std::slice::from_ref(&f1)).unwrap(),
            h[6..12]
        );
        // 前两段占用 → [12..18]
        let f2 = proposal::proposal_filename("2026-09-25", "s", &h[6..12]);
        assert_eq!(pick_shortid(&h, &[f1, f2]).unwrap(), h[12..18]);
        // 其他 source/日期的文件名不干扰
        assert_eq!(
            pick_shortid(&h, &["2026-09-25-other-zzzzzz.md".to_string()]).unwrap(),
            h[0..6]
        );
        // 候选耗尽 → E_BAD_ARGS
        let all: Vec<String> = proposal::shortid_candidates(&h)
            .iter()
            .map(|s| proposal::proposal_filename("2026-09-25", "s", s))
            .collect();
        let err = pick_shortid(&h, &all).unwrap_err();
        assert_err(&err, "E_BAD_ARGS", 2);
    }

    // ---------- check_journal ----------

    #[test]
    fn journal_guard_paths() {
        let d = GuardLimits::default();
        assert!(check_journal("choose-you", "捕捉 3 / 逃走 2", &d, 0).is_ok());
        // 1 source 格式
        let err = check_journal("Bad", "t", &d, 0).unwrap_err();
        assert_err(&err, "E_BAD_SOURCE", 2);
        // 5 text > 4000
        let err = check_journal("s", &"字".repeat(4001), &d, 0).unwrap_err();
        assert_eq!(
            err,
            DexError::TooLarge {
                what: "正文",
                limit: 4000,
                actual: 4001
            }
        );
        assert_err(&err, "E_TOO_LARGE", 5);
        // 6 限流边界：59 过 / 60 拒（journal_per_day 缺省 60）
        assert!(check_journal("s", "t", &d, 59).is_ok());
        let err = check_journal("s", "t", &d, 60).unwrap_err();
        assert_eq!(
            err,
            DexError::RateLimit {
                source: "s".to_string(),
                limit: 60
            }
        );
        assert_err(&err, "E_RATE_LIMIT", 5);
        // 8 密钥：默认拒 / advisory 放行
        let err = check_journal("s", "AKIAIOSFODNN7EXAMPLE", &d, 0).unwrap_err();
        assert_err(&err, "E_SECRET", 9);
        let adv = GuardLimits {
            secret_advisory: true,
            ..GuardLimits::default()
        };
        assert!(check_journal("s", "AKIAIOSFODNN7EXAMPLE", &adv, 0).is_ok());
    }

    // ---------- scan_secrets ----------

    #[test]
    fn secrets_each_pattern_positive() {
        let cases: Vec<(&str, String)> = vec![
            ("aws-access-key", "AKIAIOSFODNN7EXAMPLE".to_string()),
            ("github-token", format!("ghp_{}", "a".repeat(36))),
            ("slack-token", "xoxb-1234567890-abcdef".to_string()),
            ("private-key", "-----BEGIN RSA PRIVATE KEY-----".to_string()),
            ("google-api-key", format!("AIza{}", "A".repeat(35))),
            (
                "generic-api-key",
                "api_key = a1B2c3D4e5F6g7H8i9J0k1L2".to_string(),
            ),
            (
                "generic-api-key",
                "token: \"abcdefghijklmnopqrstuvwxyz123456\"".to_string(),
            ),
        ];
        for (name, text) in &cases {
            assert_eq!(
                scan_secrets(text),
                vec![*name],
                "模式 {name} 应命中：{text}"
            );
        }
    }

    #[test]
    fn secrets_clean_text_and_multi_dedup() {
        assert!(
            scan_secrets("今天聊了架构设计；周报改周四下午。普通文本 123，无密钥。").is_empty()
        );
        assert!(scan_secrets("").is_empty());
        // 同模式多次命中去重
        assert_eq!(
            scan_secrets("AKIAIOSFODNN7EXAMPLE 与 AKIAIOSFODNN7EXAMPLF"),
            vec!["aws-access-key"]
        );
        // 多模式命中：按模式表顺序返回
        let gh = format!("ghp_{}", "a".repeat(36));
        let both = format!("AKIAIOSFODNN7EXAMPLE 与 {gh}");
        assert_eq!(scan_secrets(&both), vec!["aws-access-key", "github-token"]);
    }
}
