//! §2.2 条目文件解析：列表项/段落条目、src/superseded-by/keep-until 两级作用域注释。
//!
//! 冻结语义（FR-2.2/2.5/2.10，DESIGN §2.2）：
//! - 条目单元 = 列表项行（`-`/`*`/`+` 开头；缩进续行并入该条目）**或**独立段落（连续非空行一组）
//! - H2/H3 为文件内分组；`topic` 恒取 H1
//! - 注释两级作用域：**条目级**（紧随条目行/段落末行之后，或与正文同行尾）与
//!   **文件级**（紧随 H1 标题行之后，对该文件全部条目生效）——条目级优先，两级均无 = 手写
//! - `<!-- superseded-by: <目标> -->`：被标注条目不参与注入（FR-2.5）
//! - `<!-- keep-until: YYYY-MM-DD 原因 -->`：衰减豁免（FR-2.10），两级作用域同上
//! - 文件以 `---` frontmatter 开头时跳过 frontmatter 块（残留由 lint 报告，FR-2.3）
//!
//! **B1 任务（TDD）**：先写失败测试再实现 [`parse_file`]；公共 API 冻结，可加私有助手。

use serde::{Deserialize, Serialize};

/// keep-until 豁免（FR-2.10）：`<!-- keep-until: YYYY-MM-DD 原因 -->`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeepUntil {
    /// 到期日 `YYYY-MM-DD`（非法日期格式由 lint 报告；解析层宽松接受）
    pub until: String,
    pub reason: String,
}

/// 一条记忆条目（解析产物）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// 仓库相对路径（posix 风格字符串）
    pub path: String,
    /// 条目起始行（1-based）
    pub line: usize,
    /// H1 主题
    pub topic: String,
    /// 条目正文（不含任何注释；列表项保留 `- ` markup）
    pub content: String,
    /// 无生效 src 注释（条目级优先于文件级）＝ 手写
    pub handwritten: bool,
    /// `<!-- superseded-by: X -->`（两级作用域，条目级优先）
    pub superseded_by: Option<String>,
    /// 生效 keep-until（两级作用域，条目级优先）
    pub keep_until: Option<KeepUntil>,
}

/// 条目/文件级注释累积（同一 key 后出现者覆盖先出现者——「各键取最近一条」）。
#[derive(Default)]
struct Annotations {
    src: Option<String>,
    superseded_by: Option<String>,
    /// 外层 `Some` = 该作用域存在 keep-until 注释；内层为解析结果
    /// （值非法时内层 `None`，用于阻断向文件级回退——条目级已显式覆盖）。
    keep_until: Option<Option<KeepUntil>>,
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn starts_with_ws(line: &str) -> bool {
    line.chars().next().is_some_and(char::is_whitespace)
}

/// 标题层级：行首连续 `#` 后接空格或行尾（`#tag` 无空格不算标题）。
fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if hashes == 0 {
        return None;
    }
    let rest = &t[hashes..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some(hashes)
    } else {
        None
    }
}

fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ")
}

/// 独立注释行：整行（去首尾空白）恰为一个 `<!-- ... -->`。
fn is_standalone_comment(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 7 && t.starts_with("<!--") && t.ends_with("-->")
}

/// 取独立注释行内部文本（调用方保证已过 [`is_standalone_comment`]）。
fn comment_inner(standalone: &str) -> &str {
    &standalone[4..standalone.len() - 3]
}

/// `key: value` 宽松解析：key 大小写不敏感、冒号前后空白任意；非该 key → None。
fn parse_key_value(inner: &str, key: &str) -> Option<String> {
    let (k, v) = inner.split_once(':')?;
    if k.trim().eq_ignore_ascii_case(key) {
        Some(v.trim().to_string())
    } else {
        None
    }
}

/// 把注释内部文本中已知的三个 key 吸收进累积器。
fn absorb_inner(ann: &mut Annotations, inner: &str) {
    if let Some(v) = parse_key_value(inner, "src") {
        ann.src = Some(v);
    }
    if let Some(v) = parse_key_value(inner, "superseded-by") {
        ann.superseded_by = Some(v);
    }
    if let Some(v) = parse_key_value(inner, "keep-until") {
        ann.keep_until = Some(parse_keep_until(&v));
    }
}

/// 行内容入列：剥离行尾 `<!-- ... -->` 注释（吸收已知 key），注释文本不入 content。
fn push_content_line(out: &mut Vec<String>, ann: &mut Annotations, line: &str) {
    match split_trailing_comment(line) {
        Some((prefix, inner)) => {
            absorb_inner(ann, inner);
            out.push(prefix.trim().to_string());
        }
        None => out.push(line.trim().to_string()),
    }
}

/// 找到处于行尾（其后仅余空白）的 `<!-- ... -->`，返回（注释前文本, 注释内部文本）。
/// 行中更早出现的非行尾注释不匹配；无行尾注释 → None。
fn split_trailing_comment(line: &str) -> Option<(&str, &str)> {
    let mut search = 0;
    loop {
        let start = search + line[search..].find("<!--")?;
        let after = &line[start + 4..];
        let end = after.find("-->")?;
        if after[end + 3..].trim().is_empty() {
            return Some((&line[..start], &after[..end]));
        }
        search = start + 4 + end + 3;
    }
}

/// 日期段形如 YYYY-MM-DD（10 位数字/连字符形状；日历合法性由 decay 的 NaiveDate 解析与 lint 判定）。
fn is_ymd_shape(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[..4].iter().all(|x| x.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|x| x.is_ascii_digit())
        && b[7] == b'-'
        && b[8..].iter().all(|x| x.is_ascii_digit())
}

/// 解析单个 markdown 文件为条目列表（空文件/无条目 → 空向量）。
pub fn parse_file(path: &str, content: &str) -> Vec<Entry> {
    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();

    // frontmatter 残留：文件以 `---` 开头 → 跳过至下一个 `---` 行（含）；未闭合 → 全部跳过。
    let mut start = 0;
    if n > 0 && lines[0].trim() == "---" {
        let mut close = 1;
        while close < n && lines[close].trim() != "---" {
            close += 1;
        }
        start = if close < n { close + 1 } else { n };
    }

    // H1 = 首个 `# ` 行；文件级注释 = 紧随其后的连续独立注释行（各键取最近一条）。
    let mut topic = String::new();
    let mut file_ann = Annotations::default();
    for idx in start..n {
        if heading_level(lines[idx]) == Some(1) {
            topic = lines[idx].trim_start()[1..].trim().to_string();
            let mut j = idx + 1;
            while j < n && is_standalone_comment(lines[j]) {
                absorb_inner(&mut file_ann, comment_inner(lines[j].trim()));
                j += 1;
            }
            break;
        }
    }

    let mut entries = Vec::new();
    let mut i = start;
    while i < n {
        let line = lines[i];
        if is_blank(line) || heading_level(line).is_some() || is_standalone_comment(line) {
            // 空行 / 标题（分组边界）/ 未挂靠条目的游离注释：均不成条目
            i += 1;
            continue;
        }

        let mut ann = Annotations::default();
        let mut content_lines: Vec<String> = Vec::new();
        let entry_line = i + 1;

        if is_list_item(line) {
            push_content_line(&mut content_lines, &mut ann, line);
            i += 1;
            // 缩进续行：以空白开头、非空、非注释、非新列表项、非标题 → 并入该条目
            while i < n {
                let c = lines[i];
                if is_blank(c)
                    || !starts_with_ws(c)
                    || is_standalone_comment(c)
                    || is_list_item(c)
                    || heading_level(c).is_some()
                {
                    break;
                }
                push_content_line(&mut content_lines, &mut ann, c);
                i += 1;
            }
        } else {
            // 段落：连续非空、非标题、非列表项、非注释行一组（首行行号为 entry.line）
            while i < n {
                let p = lines[i];
                if is_blank(p)
                    || heading_level(p).is_some()
                    || is_list_item(p)
                    || is_standalone_comment(p)
                {
                    break;
                }
                push_content_line(&mut content_lines, &mut ann, p);
                i += 1;
            }
        }

        // 条目级注释：紧随条目末行之后的连续独立注释行（遇空行/新条目/标题自然截止）
        while i < n && is_standalone_comment(lines[i]) {
            absorb_inner(&mut ann, comment_inner(lines[i].trim()));
            i += 1;
        }

        // 两级作用域合并：条目级优先于文件级
        entries.push(Entry {
            path: path.to_string(),
            line: entry_line,
            topic: topic.clone(),
            content: content_lines.join("\n"),
            handwritten: ann.src.or(file_ann.src.clone()).is_none(),
            superseded_by: ann.superseded_by.or(file_ann.superseded_by.clone()),
            keep_until: ann.keep_until.or(file_ann.keep_until.clone()).flatten(),
        });
    }
    entries
}

/// 从一行文本中提取行尾注释 `<!-- key: value -->`（无则 None）。
/// 公开供 lint 复用（注释格式校验，§5.6 流转层）。
pub fn trailing_comment(line: &str, key: &str) -> Option<String> {
    parse_key_value(split_trailing_comment(line)?.1, key)
}

/// 解析独立注释行 `<!-- key: value -->`（非注释行 → None）。
pub fn comment_line(line: &str, key: &str) -> Option<String> {
    if !is_standalone_comment(line) {
        return None;
    }
    parse_key_value(comment_inner(line.trim()), key)
}

/// 解析 keep-until 值 `YYYY-MM-DD 原因`（日期与原因以第一个空格分隔；无原因段允许为空）。
pub fn parse_keep_until(value: &str) -> Option<KeepUntil> {
    let v = value.trim();
    let (date, reason) = match v.find(char::is_whitespace) {
        Some(pos) => (&v[..pos], v[pos..].trim()),
        None => (v, ""),
    };
    if !is_ymd_shape(date) {
        return None;
    }
    Some(KeepUntil {
        until: date.to_string(),
        reason: reason.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- parse_file：条目单元 ----------

    #[test]
    fn parse_file_list_items_basic() {
        let md = "# 偏好\n- 周报类任务多在周四下午被提到\n- 中文写作避免「进行」一类冗词\n";
        let es = parse_file("person/preferences.md", md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].path, "person/preferences.md");
        assert_eq!(es[0].line, 2);
        assert_eq!(es[0].topic, "偏好");
        assert_eq!(es[0].content, "- 周报类任务多在周四下午被提到");
        assert!(es[0].handwritten);
        assert_eq!(es[0].superseded_by, None);
        assert_eq!(es[0].keep_until, None);
        assert_eq!(es[1].line, 3);
        assert!(es[1].handwritten);
    }

    #[test]
    fn parse_file_star_and_plus_markers() {
        let md = "# T\n* alpha\n+ beta\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].content, "* alpha");
        assert_eq!(es[1].content, "+ beta");
    }

    #[test]
    fn parse_file_paragraphs() {
        let md = "# T\nfirst line 1\nfirst line 2\n\nsecond para\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].line, 2);
        assert_eq!(es[0].content, "first line 1\nfirst line 2");
        assert_eq!(es[1].line, 5);
        assert_eq!(es[1].content, "second para");
    }

    #[test]
    fn parse_file_h2_h3_grouping_keeps_h1_topic() {
        let md = "# T\n## A\npara under A\n### B\n- list under B\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].topic, "T");
        assert_eq!(es[0].line, 3);
        assert_eq!(es[0].content, "para under A");
        assert_eq!(es[1].topic, "T");
        assert_eq!(es[1].line, 5);
        assert_eq!(es[1].content, "- list under B");
    }

    #[test]
    fn parse_file_continuation_lines_join_list_item() {
        let md = "# T\n- item first\n  continued here\n- next\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].line, 2);
        assert_eq!(es[0].content, "- item first\ncontinued here");
        assert_eq!(es[1].content, "- next");
    }

    // ---------- parse_file：两级作用域注释 ----------

    #[test]
    fn parse_file_file_level_src_marks_all_entries() {
        let md = "# 偏好\n<!-- src: choose-you 固化 2026-09 · 证据×6 -->\n- a\n- b\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert!(!es[0].handwritten);
        assert!(!es[1].handwritten);
        assert_eq!(es[0].content, "- a"); // 注释行不入 content
    }

    #[test]
    fn parse_file_file_level_multi_comment_last_occurrence_wins() {
        let md = "# T\n<!-- keep-until: 2026-01-01 first -->\n<!-- keep-until: 2026-12-31 second -->\n- a\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        let ku = es[0].keep_until.as_ref().unwrap();
        assert_eq!(ku.until, "2026-12-31");
        assert_eq!(ku.reason, "second");
    }

    #[test]
    fn parse_file_entry_level_src_mixed_file() {
        let md = "# T\n- a\n<!-- src: journal 2026-09-01 -->\n- b\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert!(!es[0].handwritten); // 紧随注释归属 a
        assert!(es[1].handwritten); // 混合文件按条目判定
        assert_eq!(es[1].content, "- b");
    }

    #[test]
    fn parse_file_trailing_and_following_comments_both_entry_level() {
        let md = "# T\n- a <!-- src: journal 2026-09-01 -->\n- b\n<!-- src: journal 2026-09-02 -->\n- c\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 3);
        assert!(!es[0].handwritten); // 行尾同行注释
        assert_eq!(es[0].content, "- a"); // 注释文本不入 content
        assert!(!es[1].handwritten); // 紧随行注释
        assert_eq!(es[1].content, "- b");
        assert!(es[2].handwritten);
    }

    #[test]
    fn parse_file_superseded_by_entry_level() {
        let md = "# T\n- old\n<!-- superseded-by: person/preferences.md#周报 -->\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        assert_eq!(
            es[0].superseded_by.as_deref(),
            Some("person/preferences.md#周报")
        );
    }

    #[test]
    fn parse_file_superseded_by_file_level() {
        let md = "# T\n<!-- superseded-by: other.md -->\n- a\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].superseded_by.as_deref(), Some("other.md"));
    }

    #[test]
    fn parse_file_keep_until_scopes_and_precedence() {
        let md = "# T\n<!-- keep-until: 2026-12-15 file level reason -->\n- a\n- b\n<!-- keep-until: 2026-06-01 entry reason -->\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        let ka = es[0].keep_until.as_ref().unwrap();
        assert_eq!(
            (ka.until.as_str(), ka.reason.as_str()),
            ("2026-12-15", "file level reason")
        );
        let kb = es[1].keep_until.as_ref().unwrap();
        assert_eq!(
            (kb.until.as_str(), kb.reason.as_str()),
            ("2026-06-01", "entry reason")
        );
    }

    #[test]
    fn parse_file_blank_line_breaks_annotation_attachment() {
        let md = "# T\n- a\n\n<!-- src: x -->\n- b\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert!(es[0].handwritten); // 空行截止：注释不归属 a
        assert!(es[1].handwritten); // 注释在 b 之前而非紧随其后：不归属 b
    }

    #[test]
    fn parse_file_comment_breaks_paragraph_continuity() {
        let md = "# T\npara one\n<!-- src: x -->\npara two\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 2);
        assert!(!es[0].handwritten);
        assert_eq!(es[0].content, "para one");
        assert!(es[1].handwritten);
        assert_eq!(es[1].content, "para two");
    }

    #[test]
    fn parse_file_unknown_key_comment_ignored() {
        let md = "# T\n- a\n<!-- note: hello -->\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        assert!(es[0].handwritten); // 非已知 key 不视为 src
        assert_eq!(es[0].content, "- a");
    }

    #[test]
    fn parse_file_paragraph_trailing_comment_stripped() {
        let md = "# T\npara content <!-- src: x -->\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].content, "para content");
        assert!(!es[0].handwritten);
    }

    // ---------- parse_file：frontmatter / 空文件 ----------

    #[test]
    fn parse_file_frontmatter_residue_skipped() {
        let md = "---\nsource: choose-you\nkind: fact\n---\n# T\n- a\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].topic, "T");
        assert_eq!(es[0].line, 6); // 行号按原文件计（含 frontmatter）
    }

    #[test]
    fn parse_file_unclosed_frontmatter_yields_no_entries() {
        let md = "---\nsource: x\n";
        assert!(parse_file("p.md", md).is_empty());
    }

    #[test]
    fn parse_file_empty_and_heading_only() {
        assert!(parse_file("p.md", "").is_empty());
        assert!(parse_file("p.md", "# T\n").is_empty());
        assert!(parse_file("p.md", "\n\n\n").is_empty());
    }

    #[test]
    fn parse_file_no_h1_empty_topic() {
        let md = "- a\n";
        let es = parse_file("p.md", md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].topic, "");
    }

    // ---------- comment_line ----------

    #[test]
    fn comment_line_basic_and_loose_matching() {
        assert_eq!(comment_line("<!-- src: x -->", "src").as_deref(), Some("x"));
        assert_eq!(comment_line("  <!--SRC:y-->", "src").as_deref(), Some("y"));
        assert_eq!(
            comment_line("<!-- Keep-Until : 2026-01-01 r -->", "keep-until").as_deref(),
            Some("2026-01-01 r")
        );
    }

    #[test]
    fn comment_line_rejects_non_standalone_or_other_key() {
        assert_eq!(comment_line("text <!-- src: x -->", "src"), None);
        assert_eq!(comment_line("<!-- other: x -->", "src"), None);
        assert_eq!(comment_line("plain text", "src"), None);
        assert_eq!(comment_line("<!-- no colon here -->", "src"), None);
        assert_eq!(comment_line("<!-- src -->", "src"), None);
    }

    // ---------- trailing_comment ----------

    #[test]
    fn trailing_comment_extracts_at_line_end() {
        assert_eq!(
            trailing_comment("text <!-- src: x -->", "src").as_deref(),
            Some("x")
        );
        assert_eq!(
            trailing_comment("text <!-- src: x -->   ", "src").as_deref(),
            Some("x")
        );
        // 非行尾的前一个注释不算，取真正处于行尾的注释
        assert_eq!(
            trailing_comment("a <!-- n: 1 --> b <!-- src: y -->", "src").as_deref(),
            Some("y")
        );
    }

    #[test]
    fn trailing_comment_rejects_missing_or_not_at_end() {
        assert_eq!(trailing_comment("text", "src"), None);
        assert_eq!(trailing_comment("text <!-- src: x --> tail", "src"), None);
        assert_eq!(trailing_comment("text <!-- src: x", "src"), None);
        assert_eq!(trailing_comment("text <!-- other: x -->", "src"), None);
    }

    // ---------- parse_keep_until ----------

    #[test]
    fn parse_keep_until_date_and_reason() {
        let ku = parse_keep_until("2026-12-15 预期重启该项目").unwrap();
        assert_eq!(ku.until, "2026-12-15");
        assert_eq!(ku.reason, "预期重启该项目");
        let ku2 = parse_keep_until("2026-12-15").unwrap();
        assert_eq!(ku2.until, "2026-12-15");
        assert_eq!(ku2.reason, "");
    }

    #[test]
    fn parse_keep_until_rejects_bad_shapes() {
        assert_eq!(parse_keep_until("tomorrow restart"), None);
        assert_eq!(parse_keep_until("2026-1-5 x"), None);
        assert_eq!(parse_keep_until(""), None);
        // 形似但日历上不存在的日期（2026-02-30）：解析层宽松接受，由 decay/lint 判定
        assert!(parse_keep_until("2026-02-30 x").is_some());
    }
}
