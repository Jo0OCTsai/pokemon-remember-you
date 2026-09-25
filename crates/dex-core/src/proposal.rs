//! §2.3 inbox 提案文件 + §2.4 journal 每日页：frontmatter 生成/剥离往返、
//! shortid/幂等 hash、journal 小节定位与追加。
//!
//! 冻结语义：
//! - frontmatter 字段：source（必填）kind（fact|preference|pattern，必填）
//!   confidence（0–100 可选缺省 50）evidence（必填）；文件体 = frontmatter + 正文
//! - 幂等 hash = SHA-256(`{source}{kind}{content}`)（confidence/evidence 不入域，§5.5-7）；
//!   shortid = hash 十六进制前 6 位，同日同 source 文件名前缀碰撞 ⇒ 顺延取后续 6 位段，
//!   仍碰撞 → E_BAD_ARGS（§2.3）
//! - 文件名：`{YYYY-MM-DD}-{source}-{shortid}.md`
//! - journal 页 `journal/{date}.md`：H1 = 日期；供稿小节 `## 供稿 · {source}`；
//!   追加**只进该小节尾部**（小节不存在则建于文件末尾；缺页建页；不跨小节覆盖，FR-5.2）
//!
//! **B3 任务（TDD）**：先写失败测试再实现；公共 API 冻结。

use crate::errors::DexError;
use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// source 标识正则（§2.3）：`^[a-z0-9][a-z0-9-]{0,31}$`
pub const SOURCE_RE: &str = r"^[a-z0-9][a-z0-9-]{0,31}$";
/// kind 枚举（§2.3）
pub const KINDS: &[&str] = &["fact", "preference", "pattern"];
/// 正文上限（Unicode 字符数，FR-4.3）
pub const CONTENT_MAX_CHARS: usize = 4000;
/// evidence 上限（Unicode 字符数；locator＋摘录合计同限，FR-4.1）
pub const EVIDENCE_MAX_CHARS: usize = 2000;
/// confidence 缺省（§2.3）
pub const DEFAULT_CONFIDENCE: i64 = 50;

/// 提案元数据（frontmatter 载荷）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalMeta {
    pub source: String,
    pub kind: String,
    pub confidence: Option<i64>,
    pub evidence: String,
}

/// 校验 source 标识格式（守卫步骤 1 的格式半边；绑定判定在 guard/runtime）。
pub fn validate_source_format(source: &str) -> Result<(), DexError> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(SOURCE_RE).expect("SOURCE_RE 为冻结常量正则，必合法"));
    if re.is_match(source) {
        Ok(())
    } else {
        Err(DexError::BadSource {
            source: source.to_string(),
        })
    }
}

/// 幂等 hash：SHA-256(`{source}{kind}{content}`) 十六进制（64 字符）。
pub fn proposal_hash(source: &str, kind: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hasher.update(kind.as_bytes());
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

/// shortid 候选序列：`hash[0..6]`、`hash[6..12]`、`hash[12..18]` …（不足 6 取剩余；耗尽即空尾）。
pub fn shortid_candidates(hash: &str) -> Vec<String> {
    let chars: Vec<char> = hash.chars().collect();
    chars.chunks(6).map(|c| c.iter().collect()).collect()
}

/// 提案文件名：`{date}-{source}-{shortid}.md`
pub fn proposal_filename(date: &str, source: &str, shortid: &str) -> String {
    format!("{date}-{source}-{shortid}.md")
}

/// 从文件名解析 `(date, source)`（不匹配提案命名式 → None；供限流计数与 review）。
/// 严格反解析：date 须恰为 YYYY-MM-DD（字符形态 + 日历校验），source 须匹配
/// [`SOURCE_RE`]（可含连字符 → 取最后一个 `-` 之前的最长段），shortid = 残余段 ≥1 字符。
pub fn parse_filename_date_source(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".md")?;
    let date = stem.get(0..10)?;
    let b = date.as_bytes();
    let shape = b.len() == 10
        && b[0..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit);
    if !shape {
        return None;
    }
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let rest = stem[10..].strip_prefix('-')?;
    let (source, shortid) = rest.rsplit_once('-')?;
    if shortid.is_empty() {
        return None;
    }
    validate_source_format(source).ok()?;
    Some((date.to_string(), source.to_string()))
}

/// 生成提案文件全文（frontmatter + 正文）。
pub fn render_proposal_file(meta: &ProposalMeta, content: &str) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("source: {}\n", meta.source));
    out.push_str(&format!("kind: {}\n", meta.kind));
    if let Some(c) = meta.confidence {
        out.push_str(&format!("confidence: {c}\n"));
    }
    out.push_str(&format!("evidence: {}\n", encode_evidence(&meta.evidence)));
    out.push_str("---\n");
    out.push_str(content);
    out
}

/// 解析提案文件全文 → (meta, content)；无/坏 frontmatter → E_BAD_ARGS。
/// 与 [`render_proposal_file`] 满足往返性质：`parse(render(m, c)) == (m*, c)`（confidence 缺省回填 50）。
pub fn parse_proposal_file(raw: &str) -> Result<(ProposalMeta, String), DexError> {
    let bad = |msg: &str| DexError::BadArgs {
        message: format!("提案文件解析失败：{msg}"),
    };
    let (first, mut pos) = line_at(raw, 0);
    if first != "---" {
        return Err(bad("首行须为 frontmatter 定界符 ---"));
    }
    let mut source: Option<String> = None;
    let mut kind: Option<String> = None;
    let mut confidence: Option<i64> = None;
    let mut evidence: Option<String> = None;
    while pos <= raw.len() {
        let (line, next) = line_at(raw, pos);
        if line == "---" {
            pos = next;
            let source = source
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("缺 source（必填）"))?;
            let kind = kind
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("缺 kind（必填）"))?;
            if !KINDS.contains(&kind.as_str()) {
                return Err(bad("kind 非枚举（fact|preference|pattern）"));
            }
            let evidence = evidence
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("缺 evidence（必填）"))?;
            let meta = ProposalMeta {
                source,
                kind,
                confidence: Some(confidence.unwrap_or(DEFAULT_CONFIDENCE)),
                evidence,
            };
            return Ok((meta, raw[pos..].to_string()));
        }
        if next == pos {
            return Err(bad("frontmatter 未闭合（缺结尾 ---）"));
        }
        if line.trim().is_empty() {
            return Err(bad("frontmatter 含空行"));
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| bad("frontmatter 行须为 key: value 结构"))?;
        let value = value.strip_prefix(' ').unwrap_or(value);
        match key {
            "source" => {
                if source.is_some() {
                    return Err(bad("source 字段重复"));
                }
                source = Some(value.to_string());
            }
            "kind" => {
                if kind.is_some() {
                    return Err(bad("kind 字段重复"));
                }
                kind = Some(value.to_string());
            }
            "confidence" => {
                if confidence.is_some() {
                    return Err(bad("confidence 字段重复"));
                }
                confidence = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| bad("confidence 须为 0–100 整数"))?,
                );
            }
            "evidence" => {
                if evidence.is_some() {
                    return Err(bad("evidence 字段重复"));
                }
                let decoded = if value.starts_with('"') {
                    unquote_basic(value).map_err(|m| bad(&m))?
                } else {
                    value.to_string()
                };
                evidence = Some(decoded);
            }
            _ => return Err(bad(&format!("未知 frontmatter 字段：{key:?}"))),
        }
        pos = next;
    }
    unreachable!("循环出口必经 break（上闭区间界）")
}

/// journal 页相对路径：`journal/{date}.md`
pub fn journal_page_path(date: &str) -> String {
    format!("journal/{date}.md")
}

/// 供稿小节标题行：`## 供稿 · {source}`
pub fn journal_section_title(source: &str) -> String {
    format!("## 供稿 · {source}")
}

/// journal 追加（§2.4）：正文（每条一行 `- ` 列表项）追加至 `## 供稿 · {source}` 小节尾部；
/// 小节不存在则建于文件末（保持空行分隔）；页面为空/缺页 → 建页（H1 = date，先供稿小节）。
/// 返回追加后的页面全文（不跨小节覆盖；同日重复供稿追加至小节尾部）。
pub fn append_journal(page: &str, date: &str, source: &str, text: &str) -> String {
    let title = journal_section_title(source);
    let items: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| format!("- {l}"))
        .collect();
    // 页面为空/缺页 → 建页：H1 + 空行 + 供稿小节
    if page.trim().is_empty() {
        let mut out = format!("# {date}\n\n{title}\n");
        for item in &items {
            out.push_str(item);
            out.push('\n');
        }
        return out;
    }
    // 定位供稿小节（整行精确匹配）
    let mut pos = 0;
    while pos < page.len() {
        let (line, next) = line_at(page, pos);
        if line == title {
            return append_within_section(page, next, &items);
        }
        pos = next;
    }
    // 小节不存在 → 建于文件末（空行分隔；不动既有小节——含「手写」）
    let base = page.trim_end_matches(['\n', '\r']);
    let mut out = format!("{base}\n\n{title}\n");
    for item in &items {
        out.push_str(item);
        out.push('\n');
    }
    out
}

/// 在已定位的小节（`after_title` = 标题行之后的首字节）尾部追加列表项：
/// 尾部 = 下一个 `## ` 小节行之前或 EOF；剥除小节内尾部空行后插入，保留原有小节间分隔与 EOF 尾换行。
fn append_within_section(page: &str, after_title: usize, items: &[String]) -> String {
    let mut tail = page.len();
    let mut q = after_title;
    while q < page.len() {
        let (line, next) = line_at(page, q);
        if line.starts_with("## ") {
            tail = q;
            break;
        }
        q = next;
    }
    let body = &page[after_title..tail];
    let body_end = after_title + body.trim_end().len();
    let prefix = &page[..body_end];
    let suffix = &page[body_end..];
    if items.is_empty() {
        return page.to_string();
    }
    let mut insert = String::new();
    if !prefix.ends_with('\n') {
        insert.push('\n');
    }
    insert.push_str(&items.join("\n"));
    if suffix.is_empty() {
        insert.push('\n');
    }
    format!("{prefix}{insert}{suffix}")
}

/// 取 `s` 中 `pos` 起的一行：返回（不含行尾 `\n` / `\r\n` 的行, 下一行起始字节下标）。
fn line_at(s: &str, pos: usize) -> (&str, usize) {
    match s[pos..].find('\n') {
        Some(i) => {
            let end = pos + i;
            (
                s[pos..end].strip_suffix('\r').unwrap_or(&s[pos..end]),
                end + 1,
            )
        }
        None => (&s[pos..], s.len()),
    }
}

/// evidence 值编码：单行且无特殊字符 → 原样；否则 TOML 基本字符串（`"` + 转义），
/// 与 [`unquote_basic`] 成对，保证往返无损。
fn encode_evidence(v: &str) -> String {
    let needs_quote = v.is_empty()
        || v.starts_with('"')
        || v.contains('\n')
        || v.contains('\r')
        || v.contains('\\')
        || v.contains('\t')
        || v.chars().any(|c| c.is_control());
    if !needs_quote {
        return v.to_string();
    }
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 解码 TOML 基本字符串（`"…"`，含 `\\`、`\"`、`\n`、`\r`、`\t`、`\b`、`\f`、`\uXXXX`）。
fn unquote_basic(v: &str) -> Result<String, String> {
    let inner = v.strip_prefix('"').ok_or("evidence 引号字符串缺开头引号")?;
    let inner = inner
        .strip_suffix('"')
        .ok_or("evidence 引号字符串缺结尾引号")?;
    let mut out = String::with_capacity(inner.len());
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                if hex.len() != 4 {
                    return Err("evidence \\u 转义须为 4 位十六进制".to_string());
                }
                let cp = u32::from_str_radix(&hex, 16)
                    .map_err(|_| "evidence \\u 转义非 4 位十六进制".to_string())?;
                out.push(char::from_u32(cp).ok_or("evidence \\u 转义非有效码点".to_string())?);
            }
            _ => return Err("evidence 含非法转义".to_string()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_err(err: &DexError, code: &str, exit: i32) {
        assert_eq!(err.code(), code);
        assert_eq!(err.exit_code(), exit);
    }

    fn meta(source: &str, kind: &str, confidence: Option<i64>, evidence: &str) -> ProposalMeta {
        ProposalMeta {
            source: source.to_string(),
            kind: kind.to_string(),
            confidence,
            evidence: evidence.to_string(),
        }
    }

    // ---------- validate_source_format ----------

    #[test]
    fn source_format_accepts_and_rejects() {
        for ok in ["a", "choose-you", "0-x", "2026-app", &"a".repeat(32)] {
            assert!(validate_source_format(ok).is_ok(), "应合法：{ok}");
        }
        for bad in [
            "",
            "-a",
            "A",
            "Choose",
            "choose_you",
            "choose you",
            &"a".repeat(33),
            "你号",
        ] {
            assert_eq!(
                validate_source_format(bad),
                Err(DexError::BadSource {
                    source: bad.to_string()
                }),
                "应非法：{bad:?}"
            );
        }
    }

    // ---------- proposal_hash / shortid_candidates ----------

    #[test]
    fn hash_is_sha256_of_concat() {
        // SHA-256("abc") 已知向量——直接拼接语义
        assert_eq!(
            proposal_hash("a", "b", "c"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let h = proposal_hash("choose-you", "pattern", "正文内容");
        assert_eq!(h.len(), 64);
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "hex 小写：{h}"
        );
        assert_ne!(h, proposal_hash("choose-you", "pattern", "正文内容2"));
        assert_ne!(h, proposal_hash("choose-you", "fact", "正文内容"));
        assert_ne!(h, proposal_hash("other", "pattern", "正文内容"));
    }

    #[test]
    fn shortid_candidate_sequence() {
        let hash = "0123456789abcdef".repeat(4); // 64 字符
        let segs = shortid_candidates(&hash);
        assert_eq!(segs.len(), 11, "64/6 = 10 段 + 4 字符尾段");
        assert_eq!(segs[0], "012345");
        assert_eq!(segs[1], "6789ab");
        assert_eq!(segs[2], "cdef01");
        assert_eq!(segs[10], "cdef", "末段不足 6 取剩余");
        // 短输入：单段
        assert_eq!(shortid_candidates("abc"), vec!["abc"]);
    }

    // ---------- 文件名 / 路径 / 小节标题 ----------

    #[test]
    fn filename_and_paths() {
        assert_eq!(
            proposal_filename("2026-09-25", "choose-you", "a1b2c3"),
            "2026-09-25-choose-you-a1b2c3.md"
        );
        assert_eq!(journal_page_path("2026-09-25"), "journal/2026-09-25.md");
        assert_eq!(journal_section_title("choose-you"), "## 供稿 · choose-you");
    }

    #[test]
    fn parse_filename_roundtrip() {
        assert_eq!(
            parse_filename_date_source("2026-09-25-choose-you-a1b2c3.md").unwrap(),
            ("2026-09-25".to_string(), "choose-you".to_string())
        );
        assert_eq!(
            parse_filename_date_source("2026-01-01-a-b.md").unwrap(),
            ("2026-01-01".to_string(), "a".to_string())
        );
        // 与 proposal_filename 互逆（source 含连字符也须正确归属）
        let name = proposal_filename("2026-09-25", "choose-you", "a1b2c3");
        assert_eq!(
            parse_filename_date_source(&name).unwrap(),
            ("2026-09-25".to_string(), "choose-you".to_string())
        );
        for bad in [
            "2026-09-25-choose-you-a1b2c3.txt",
            "2026-9-25-src-a1b2c3.md",
            "2026-02-30-src-a1b2c3.md",
            "2026-09-25-Src-a1b2c3.md",
            "2026-09-25-src-.md",
            "2026-09-25.md",
            "20260925-src-a1b2c3.md",
            "notes.md",
            "2026-09-25-src",
        ] {
            assert!(parse_filename_date_source(bad).is_none(), "应拒绝：{bad}");
        }
    }

    // ---------- render / parse 往返 ----------

    #[test]
    fn render_matches_design_example() {
        let m = meta(
            "choose-you",
            "pattern",
            Some(85),
            "chat_feedback #1234 #1301 #1355",
        );
        let raw = render_proposal_file(&m, "群聊「摸鱼俱乐部」的消息全为闲聊，对该用户无待办含义");
        assert_eq!(
            raw,
            "---\nsource: choose-you\nkind: pattern\nconfidence: 85\nevidence: chat_feedback #1234 #1301 #1355\n---\n群聊「摸鱼俱乐部」的消息全为闲聊，对该用户无待办含义"
        );
    }

    #[test]
    fn roundtrip_byte_equal_canonical() {
        // 规范形（confidence 显式给定）→ 生成→解析→再生成字节相等；
        // evidence/content 覆盖引号、井号、换行、制表、反斜杠、首尾空白、正文中含 --- 行
        let cases: Vec<(ProposalMeta, &str)> = vec![
            (
                meta(
                    "choose-you",
                    "fact",
                    Some(50),
                    "chat_feedback #1234 #1301 #1355",
                ),
                "群聊的消息全为闲聊，对该用户无待办含义\n",
            ),
            (
                meta("app-x", "preference", Some(0), "带 \"引号\" 与 # 井号"),
                "多行\n正文\n---\n含三横线\n",
            ),
            (
                meta("s", "pattern", Some(100), "第一行\n第二行\t制表\\反斜杠"),
                "",
            ),
            (
                meta("s2", "fact", Some(85), "尾随空格  "),
                "\n前导换行的内容\n",
            ),
            (
                meta("unicode-src", "pattern", Some(7), "证据「＃」全角🙂"),
                "内容含 emoji 🙂\n",
            ),
        ];
        for (m, c) in cases {
            let raw1 = render_proposal_file(&m, c);
            let (m2, c2) = parse_proposal_file(&raw1).unwrap();
            assert_eq!(m2, m, "解析元数据应无损：{raw1:?}");
            assert_eq!(c2, c, "解析正文应无损：{raw1:?}");
            let raw2 = render_proposal_file(&m2, &c2);
            assert_eq!(raw1, raw2, "生成→解析→再生成应字节相等");
        }
    }

    #[test]
    fn confidence_default_roundtrip() {
        let m = meta("s", "fact", None, "ev");
        let raw = render_proposal_file(&m, "c");
        assert!(!raw.contains("confidence"), "confidence 为 None 时不写该行");
        let (m2, _) = parse_proposal_file(&raw).unwrap();
        assert_eq!(m2.confidence, Some(DEFAULT_CONFIDENCE));
        // 规范化后再往返：稳定
        let raw2 = render_proposal_file(&m2, "c");
        let (m3, _) = parse_proposal_file(&raw2).unwrap();
        assert_eq!(m2, m3);
        assert_eq!(render_proposal_file(&m3, "c"), raw2);
    }

    #[test]
    fn parse_rejects_malformed() {
        let bad_files = [
            "",
            "无 frontmatter 头",
            "source: s\nkind: fact\nevidence: e\n---\n正文",
            "---\nsource: s\nkind: fact\n---\n正文",
            "---\nsource: s\nevidence: e\n---\n正文",
            "---\nkind: fact\nevidence: e\n---\n正文",
            "---\nsource: s\nkind: fact\nevidence: \n---\n正文",
            "---\nsource: s\nkind: fact\nevidence: \"\"\n---\n正文",
            "---\nsource: s\nkind: diary\nevidence: e\n---\n正文",
            "---\nsource: s\nkind: fact\nconfidence: abc\nevidence: e\n---\n正文",
            "---\nsource: s\nkind: fact\nevidence: \"未闭合\n---\n正文",
            "---\nunknown: 1\nsource: s\nkind: fact\nevidence: e\n---\n正文",
            "---\nsource: s\nkind: fact\nevidence: e",
        ];
        for bad in bad_files {
            let err = parse_proposal_file(bad).unwrap_err();
            assert_err(&err, "E_BAD_ARGS", 2);
        }
    }

    // ---------- append_journal ----------

    #[test]
    fn journal_missing_page_creates() {
        let out = append_journal("", "2026-09-20", "choose-you", "捕捉 3 / 逃走 2");
        assert_eq!(
            out,
            "# 2026-09-20\n\n## 供稿 · choose-you\n- 捕捉 3 / 逃走 2\n"
        );
        // 空白页同缺页
        assert_eq!(
            append_journal("  \n", "2026-09-20", "a", "x"),
            "# 2026-09-20\n\n## 供稿 · a\n- x\n"
        );
    }

    #[test]
    fn journal_appends_at_section_tail_before_next_section() {
        let page = "# 2026-09-20\n## 供稿 · choose-you\n- 捕捉 3\n## 手写\n- 今天定了方案\n";
        let out = append_journal(page, "2026-09-20", "choose-you", "逃走 2");
        assert_eq!(
            out,
            "# 2026-09-20\n## 供稿 · choose-you\n- 捕捉 3\n- 逃走 2\n## 手写\n- 今天定了方案\n"
        );
        // 小节间已有空行分隔：追加进小节尾部，保留空行分隔
        let page2 = "# d\n## 供稿 · a\n- x1\n\n## 手写\n- h\n";
        let out2 = append_journal(page2, "2026-09-20", "a", "t");
        assert_eq!(out2, "# d\n## 供稿 · a\n- x1\n- t\n\n## 手写\n- h\n");
    }

    #[test]
    fn journal_appends_at_eof_tail() {
        // 供稿小节在文件末尾且无尾换行：追加后补齐尾换行
        let page = "# 2026-09-20\n## 手写\n- h\n## 供稿 · choose-you\n- 第一条";
        let out = append_journal(page, "2026-09-20", "choose-you", "第二条");
        assert_eq!(
            out,
            "# 2026-09-20\n## 手写\n- h\n## 供稿 · choose-you\n- 第一条\n- 第二条\n"
        );
    }

    #[test]
    fn journal_repeat_appends_tail() {
        let p1 = append_journal("", "2026-09-20", "a", "一");
        let p2 = append_journal(&p1, "2026-09-20", "a", "二");
        let p3 = append_journal(&p2, "2026-09-20", "a", "三");
        assert_eq!(p3, "# 2026-09-20\n\n## 供稿 · a\n- 一\n- 二\n- 三\n");
    }

    #[test]
    fn journal_new_section_appended_at_end() {
        // 小节不存在（供稿 choose-you）→ 建于文件末；不碰「手写」小节（不跨小节）
        let page = "# 2026-09-20\n## 手写\n- 人写的内容\n";
        let out = append_journal(page, "2026-09-20", "choose-you", "摘要");
        assert_eq!(
            out,
            "# 2026-09-20\n## 手写\n- 人写的内容\n\n## 供稿 · choose-you\n- 摘要\n"
        );
    }

    #[test]
    fn journal_multiline_text_becomes_items() {
        let out = append_journal("", "2026-09-20", "a", "行一\n行二");
        assert_eq!(out, "# 2026-09-20\n\n## 供稿 · a\n- 行一\n- 行二\n");
    }
}
