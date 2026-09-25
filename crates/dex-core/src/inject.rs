//! §5.2 注入管线：优先级合并（字典序比较器）→ 冲突处理 → 预算截断 → 净化。
//!
//! 冻结语义（FR-3.2/3.3/6.16，DESIGN §5.2）：
//! - 字典序比较器（依序判定）：① 具体 ＞ 泛化（projects＞apps＞domains＞person）
//!   ② 手写 ＞ 固化（仅同级裁决——①先行分组后②自然只在同级生效）
//!   ③ 新证据 ＞ 旧证据（last_change 新者胜；未知日期视为最旧）
//!   ④ 终局稳定序 `path:line` 字典序（①②③全平手时保证全序——render 金样本跨运行字节一致）
//! - 冲突处理：同 topic（H1 精确相等）多条 → 仅注入最高优先级者；superseded-by 条目直接排除；
//!   suppressed 计数 = 冲突压制 + superseded 排除（不并入 omitted）
//! - 预算截断：按序累加，加入后使 entries/chars 任一越界的下一条**整条丢弃并停止累加**
//!   （输出恒为优先级前缀）；首条即越界 ⇒ 输出空 + `over_budget=true`（W_OVER_BUDGET 由调用方挂）
//! - omitted = 因预算截断丢弃的条目数（含停止累加后全部剩余）
//! - 字数口径：Unicode 字符数（`.chars().count()`），含 markdown 语法、不含注释（解析时已剥离）
//! - 净化（FR-6.16）：`<` `>` `&` HTML 转义；**预算按转义前原文计数**
//!
//! **B2 任务（TDD，性质测试）**：先写失败测试再实现；公共 API 冻结。

use crate::entry::Entry;
use crate::scope::ScopeId;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// 注入预算（FR-3.3；缺省 10 条 / 2000 字）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub entries: usize,
    pub chars: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            entries: 10,
            chars: 2000,
        }
    }
}

/// 注入候选（收集阶段的产物；specificity/handwritten 由 scope 与 entry 派生）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub entry: Entry,
    pub scope: ScopeId,
    /// 最后实质变更日 `YYYY-MM-DD`（v1 = 单遍 git log 快照，§5.4；未知 = 空串 → 最旧）
    pub last_change: String,
}

/// 注入输出条目
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InjectItem {
    pub entry: Entry,
    pub scope: ScopeId,
}

/// 管线输出
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InjectOutcome {
    /// 按优先级序的注入条目（预算内）
    pub items: Vec<InjectItem>,
    /// 因预算截断丢弃的条目数（显式告知，FR-3.3）
    pub omitted: usize,
    /// 冲突压制 + superseded 排除计数（另行附计，§5.2-4）
    pub suppressed: usize,
    /// 首条即越界（输出为空）⇒ true（调用方挂 W_OVER_BUDGET）
    pub over_budget: bool,
}

/// FR-3.2 字典序比较器（① 具体性 ② 手写＞固化 ③ 新＞旧 ④ path:line 终局序）。
/// 公开供性质测试（任意置换排序幂等、无并列不可比——④保证全序）。
pub fn compare(a: &Candidate, b: &Candidate) -> Ordering {
    // ①②③ 降序（大者优先）：specificity / handwritten（true＞false）/ last_change（新＞旧，空串最旧垫底）
    b.scope
        .specificity()
        .cmp(&a.scope.specificity())
        .then_with(|| b.entry.handwritten.cmp(&a.entry.handwritten))
        .then_with(|| b.last_change.cmp(&a.last_change))
        // ④ 终局稳定序：path 字典序后 line 数值升序（保证全序）
        .then_with(|| a.entry.path.cmp(&b.entry.path))
        .then_with(|| a.entry.line.cmp(&b.entry.line))
}

/// 注入管线（§5.2 步骤 2–4）。
pub fn pipeline(candidates: Vec<Candidate>, budget: Budget) -> InjectOutcome {
    // 步骤 1（前置过滤）：superseded-by 条目直接排除，计入 suppressed
    let mut suppressed = 0usize;
    let mut live: Vec<Candidate> = Vec::with_capacity(candidates.len());
    for c in candidates {
        if c.entry.superseded_by.is_some() {
            suppressed += 1;
        } else {
            live.push(c);
        }
    }

    // 步骤 2：字典序排序（稳定；④ 保证除 (path,line) 完全相同外全序）
    live.sort_by(compare);

    // 步骤 3：冲突处理——同 topic（H1 精确相等）只保留排序后首个，其余计入 suppressed；
    // 注入侧只做选择，不改数据（冗余留待周回顾合并）
    let mut seen_topics = std::collections::HashSet::new();
    let mut deduped: Vec<Candidate> = Vec::with_capacity(live.len());
    for c in live {
        if seen_topics.insert(c.entry.topic.clone()) {
            deduped.push(c);
        } else {
            suppressed += 1;
        }
    }

    // 步骤 4：预算截断——按序累加，越界的下一条整条丢弃并停止累加（输出恒为优先级前缀）；
    // 首条即越界 ⇒ 输出空 + over_budget=true；omitted = 截断丢弃的条目数（含停止后全部剩余）
    let total = deduped.len();
    let mut items: Vec<InjectItem> = Vec::new();
    let mut chars = 0usize;
    let mut omitted = 0usize;
    let mut over_budget = false;
    for (i, c) in deduped.into_iter().enumerate() {
        let entry_chars = char_count(&c.entry.content);
        if items.len() + 1 > budget.entries || chars + entry_chars > budget.chars {
            omitted = total - i;
            over_budget = items.is_empty();
            break;
        }
        chars += entry_chars;
        items.push(InjectItem {
            entry: c.entry,
            scope: c.scope,
        });
    }
    InjectOutcome {
        items,
        omitted,
        suppressed,
        over_budget,
    }
}

/// FR-3.3 字数口径：Unicode 字符数。
pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// FR-6.16 净化：HTML 转义 `&` `<` `>`（先计数后转义由调用方保证顺序）。
pub fn sanitize(text: &str) -> String {
    // `&` 先转：后两步引入的实体（&lt;/&gt;）不再被二次转义
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 构造助手 ----------

    fn cand(
        path: &str,
        line: usize,
        topic: &str,
        content: &str,
        handwritten: bool,
        scope_id: &str,
        last_change: &str,
    ) -> Candidate {
        Candidate {
            entry: Entry {
                path: path.to_string(),
                line,
                topic: topic.to_string(),
                content: content.to_string(),
                handwritten,
                superseded_by: None,
                keep_until: None,
            },
            scope: ScopeId::new(scope_id).unwrap(),
            last_change: last_change.to_string(),
        }
    }

    fn with_superseded(mut c: Candidate) -> Candidate {
        c.entry.superseded_by = Some("replaced".to_string());
        c
    }

    /// 确定性 LCG（固定种子，无外部依赖）
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// 随机构造候选（固定种子）：path 取小池（5 个）× line 取唯一索引 ⇒ (path, line) 全局唯一
    /// （⇒ compare 永不平手）；scope/手写/日期取小池 ⇒ ①②③ 大量平手以锻炼 ③④ 键。
    fn random_candidates(n: usize, seed: u64) -> Vec<Candidate> {
        let scopes = [
            "person",
            "domains/coding",
            "domains/work",
            "apps/todo",
            "apps/notes",
            "projects/foo",
            "projects/bar",
        ];
        let dates = ["", "", "2024-01-01", "2025-06-15", "2026-09-25"];
        let mut rng = Rng(seed);
        (0..n)
            .map(|i| {
                cand(
                    &format!("doc{}.md", i % 5),
                    i + 1,
                    &format!("topic-{i}"),
                    "body",
                    rng.below(2) == 0,
                    scopes[rng.below(scopes.len())],
                    dates[rng.below(dates.len())],
                )
            })
            .collect()
    }

    fn shuffled(orig: &[Candidate], rng: &mut Rng) -> Vec<Candidate> {
        let mut v = orig.to_vec();
        for i in (1..v.len()).rev() {
            v.swap(i, rng.below(i + 1));
        }
        v
    }

    fn contents(out: &InjectOutcome) -> Vec<&str> {
        out.items
            .iter()
            .map(|it| it.entry.content.as_str())
            .collect()
    }

    // ---------- 缺省预算（FR-3.3） ----------

    #[test]
    fn default_budget_is_10_entries_2000_chars() {
        assert_eq!(
            Budget::default(),
            Budget {
                entries: 10,
                chars: 2000
            }
        );
    }

    // ---------- 字数口径（FR-3.3：Unicode 标量、含 markup） ----------

    #[test]
    fn char_count_is_unicode_scalar_units() {
        assert_eq!(char_count(""), 0);
        assert_eq!(char_count("plain"), 5);
        assert_eq!(char_count("- 中文"), 4); // 含 `- ` 列表 markup（FR-3.3：含 markdown 语法）
        assert_eq!(char_count("📝"), 1); // emoji = 1 个 Unicode 标量
        assert_eq!(char_count("a😀b"), 3);
        assert_eq!(char_count("🇨🇳"), 2); // 旗帜 = 2 个区域指示符标量（.chars().count() 口径）
    }

    // ---------- 净化（FR-6.16） ----------

    #[test]
    fn sanitize_escapes_html() {
        assert_eq!(sanitize("<script>"), "&lt;script&gt;");
        assert_eq!(sanitize("&"), "&amp;");
        assert_eq!(sanitize("a&<b>"), "a&amp;&lt;b&gt;");
        assert_eq!(sanitize("&&"), "&amp;&amp;");
        assert_eq!(sanitize("&lt;"), "&amp;lt;"); // 字面 & 先转义、不产双逃逸
        assert_eq!(sanitize("plain 中文📝"), "plain 中文📝"); // 无特殊字符原样
    }

    // ---------- 比较器字典序判定（FR-3.2 ①②③ 分层裁决） ----------

    #[test]
    fn compare_projects_cured_beats_person_handwritten() {
        // ① 跨级具体性优先：projects 固化（最旧日期）压过 person 手写（最新日期）——① 先于 ②③
        let proj = cand("z.md", 1, "t", "c", false, "projects/foo", "2020-01-01");
        let person = cand("a.md", 1, "t", "c", true, "person", "2026-09-25");
        assert_eq!(compare(&proj, &person), Ordering::Less);
        assert_eq!(compare(&person, &proj), Ordering::Greater);
    }

    #[test]
    fn compare_same_level_handwritten_beats_cured_even_when_older() {
        // ② 仅同级裁决：手写（日期空 = 最旧）仍压过固化（最新）——② 先于 ③
        let hw = cand("z.md", 1, "t", "c", true, "domains/work", "");
        let cured = cand("a.md", 1, "t", "c", false, "domains/work", "2026-09-25");
        assert_eq!(compare(&hw, &cured), Ordering::Less);
    }

    #[test]
    fn compare_same_level_same_flag_newer_date_wins() {
        let newer = cand("z.md", 1, "t", "c", true, "person", "2026-01-01");
        let older = cand("a.md", 1, "t", "c", true, "person", "2020-01-01");
        assert_eq!(compare(&newer, &older), Ordering::Less);
    }

    #[test]
    fn compare_empty_date_is_oldest_sinks_below_any_dated() {
        // 空串 = 未知日期 = 最旧垫底：任一真实日期（哪怕 2001 年）都压过它——③ 先于 ④
        let dated = cand("z.md", 1, "t", "c", false, "apps/todo", "2001-01-01");
        let undated = cand("a.md", 1, "t", "c", false, "apps/todo", "");
        assert_eq!(compare(&dated, &undated), Ordering::Less);
        assert_eq!(compare(&undated, &dated), Ordering::Greater);
    }

    #[test]
    fn compare_path_line_final_tiebreak_numeric_line() {
        // ④ ①②③ 全平时：先 path 字典序、后 line 数值升序（10 > 2 数值序——字符串序会判反）
        let a3 = cand("a.md", 3, "t", "c", false, "person", "2025-01-01");
        let a10 = cand("a.md", 10, "t", "c", false, "person", "2025-01-01");
        let b2 = cand("b.md", 2, "t", "c", false, "person", "2025-01-01");
        let b10 = cand("b.md", 10, "t", "c", false, "person", "2025-01-01");
        assert_eq!(compare(&a3, &a10), Ordering::Less);
        assert_eq!(compare(&a10, &b2), Ordering::Less); // path 先于 line
        assert_eq!(compare(&b2, &b10), Ordering::Less);
    }

    // ---------- 比较器全序性质（固定种子 ≥30 随机候选） ----------

    #[test]
    fn comparator_total_order_property() {
        let candidates = random_candidates(40, 0x00B2_5EED);
        let mut reference = candidates.clone();
        reference.sort_by(compare);

        // sort 幂等：对已排序结果再排一次不变
        let mut again = reference.clone();
        again.sort_by(compare);
        assert_eq!(again, reference, "sort 幂等性失败");

        // 任意置换排序结果一致（全序无并列不可比；若 compare 非全序，不同置换会产生不同序）
        let mut rng = Rng(0x5EED_00B2);
        for round in 0..5 {
            let mut perm = shuffled(&candidates, &mut rng);
            perm.sort_by(compare);
            assert_eq!(perm, reference, "置换 {round} 排序结果与基准不一致");
        }

        // 排序位置与 compare 一致：(path,line) 唯一 ⇒ 全序严格 ⇒ 前位严格 Less
        for (i, a) in reference.iter().enumerate() {
            for b in &reference[i + 1..] {
                assert_eq!(
                    compare(a, b),
                    Ordering::Less,
                    "位置 {i} 应严格先于其后条目（path={}, line={} vs path={}, line={}）",
                    a.entry.path,
                    a.entry.line,
                    b.entry.path,
                    b.entry.line
                );
            }
        }
    }

    // ---------- 管线 ----------

    #[test]
    fn pipeline_empty_input_yields_empty_outcome() {
        assert_eq!(
            pipeline(vec![], Budget::default()),
            InjectOutcome::default()
        );
    }

    #[test]
    fn pipeline_orders_items_by_priority_and_carries_scope() {
        // 乱序喂入：① projects 固化 ×2（path:line 决出）→ person 手写 → person 固化
        let c = vec![
            cand("d.md", 1, "T-d", "D", false, "person", "2026-01-01"),
            cand("b.md", 9, "T-b", "B", false, "projects/foo", "2020-01-01"),
            cand("c.md", 1, "T-c", "C", true, "person", ""),
            cand("a.md", 5, "T-a", "A", false, "projects/foo", "2020-01-01"),
        ];
        let out = pipeline(c, Budget::default());
        assert_eq!(contents(&out), vec!["A", "B", "C", "D"]);
        assert_eq!(out.items[0].scope, ScopeId::new("projects/foo").unwrap());
        assert_eq!(out.items[2].scope, ScopeId::new("person").unwrap());
        assert_eq!(out.omitted, 0);
        assert_eq!(out.suppressed, 0);
        assert!(!out.over_budget);
    }

    #[test]
    fn pipeline_conflict_same_topic_keeps_highest_priority() {
        let hi = cand("a.md", 1, "same", "hi", true, "projects/foo", "2026-01-01");
        let lo = cand("b.md", 1, "same", "lo", false, "person", "");
        // 乱序喂入（低优先在前）
        let out = pipeline(vec![lo, hi], Budget::default());
        assert_eq!(contents(&out), vec!["hi"]);
        assert_eq!(out.suppressed, 1, "冲突压制计入 suppressed");
        assert_eq!(out.omitted, 0, "冲突压制不混入 omitted");
    }

    #[test]
    fn pipeline_superseded_excluded_and_counted() {
        let sup = with_superseded(cand(
            "a.md",
            1,
            "T-1",
            "sup",
            true,
            "projects/foo",
            "2026-01-01",
        ));
        let live = cand("b.md", 1, "T-2", "live", false, "person", "");
        let out = pipeline(vec![sup, live], Budget::default());
        assert_eq!(contents(&out), vec!["live"]);
        assert_eq!(out.suppressed, 1, "superseded 排除计入 suppressed");
        assert_eq!(out.omitted, 0);
    }

    #[test]
    fn pipeline_superseded_same_topic_does_not_suppress_live() {
        // superseded 在排序/冲突处理前排除：不得以其 topic 压制同 topic 存活条目
        let sup = with_superseded(cand(
            "a.md",
            1,
            "same",
            "sup",
            true,
            "projects/foo",
            "2026-01-01",
        ));
        let live = cand("b.md", 1, "same", "live", false, "person", "");
        let out = pipeline(vec![sup, live], Budget::default());
        assert_eq!(contents(&out), vec!["live"]);
        assert_eq!(out.suppressed, 1);
    }

    #[test]
    fn pipeline_omitted_and_suppressed_disjoint() {
        // 1 条冲突压制 + 1 条 superseded + 1 条 entries 截断：三类计数互不混计
        let c = vec![
            cand("a.md", 1, "T", "A", true, "projects/foo", "2026-01-01"), // 注入 1
            cand("a2.md", 1, "T", "dup", false, "person", ""),             // 同 topic → suppressed
            with_superseded(cand(
                "s.md",
                1,
                "T-s",
                "sup",
                true,
                "apps/todo",
                "2026-01-01",
            )), // suppressed
            cand("b.md", 1, "T-b", "B", false, "person", ""),              // 注入 2
            cand("c.md", 1, "T-c", "C", false, "person", ""),              // 越界 → omitted
        ];
        let out = pipeline(
            c,
            Budget {
                entries: 2,
                chars: 100,
            },
        );
        assert_eq!(contents(&out), vec!["A", "B"]);
        assert_eq!(out.suppressed, 2, "冲突 1 + superseded 1");
        assert_eq!(out.omitted, 1, "预算截断 1");
        assert!(!out.over_budget);
    }

    // ---------- 预算截断（FR-3.3 / §5.2-4） ----------

    #[test]
    fn pipeline_budget_whole_item_drop_no_refill() {
        // 第 2 条（6 字）超 chars=5 ⇒ 整条丢弃并停止累加；第 3 条（1 字）虽能续填也不补位
        let c1 = cand("a.md", 1, "T-1", "aaaa", false, "person", "");
        let c2 = cand("b.md", 1, "T-2", "bbbbbb", false, "person", "");
        let c3 = cand("c.md", 1, "T-3", "c", false, "person", "");
        let out = pipeline(
            vec![c3, c1, c2],
            Budget {
                entries: 10,
                chars: 5,
            },
        );
        assert_eq!(contents(&out), vec!["aaaa"]);
        assert_eq!(out.omitted, 2, "越界条 + 其后全部（含本可续填者）");
        assert!(!out.over_budget, "非首条越界不置 over_budget");
    }

    #[test]
    fn pipeline_first_item_over_budget_yields_empty_and_flag() {
        let big = cand("a.md", 1, "T", "0123456789", false, "person", "");
        let small = cand("b.md", 1, "T-2", "x", false, "person", "");
        let out = pipeline(
            vec![big, small],
            Budget {
                entries: 10,
                chars: 5,
            },
        );
        assert!(out.items.is_empty(), "首条即越界 ⇒ 输出空");
        assert!(
            out.over_budget,
            "首条即越界 ⇒ over_budget=true（调用方挂 W_OVER_BUDGET）"
        );
        assert_eq!(out.omitted, 2, "全部剩余计入 omitted");
        assert_eq!(out.suppressed, 0);
    }

    #[test]
    fn pipeline_entries_cap_truncates_suffix() {
        let c: Vec<Candidate> = (1..=3)
            .map(|i| {
                cand(
                    &format!("f{i}.md"),
                    1,
                    &format!("T-{i}"),
                    "x",
                    false,
                    "person",
                    "",
                )
            })
            .collect();
        let out = pipeline(
            c,
            Budget {
                entries: 2,
                chars: 100,
            },
        );
        assert_eq!(contents(&out), vec!["x", "x"]);
        assert_eq!(out.omitted, 1);
        assert!(!out.over_budget);
    }

    #[test]
    fn pipeline_unicode_chars_budget() {
        // "- 中文📝" = 5 个 Unicode 标量（含 `- ` markup）
        let c = cand("a.md", 1, "T", "- 中文📝", false, "person", "");
        let fits = pipeline(
            vec![c.clone()],
            Budget {
                entries: 10,
                chars: 5,
            },
        );
        assert_eq!(contents(&fits), vec!["- 中文📝"], "恰好 5 字不越界");
        let over = pipeline(
            vec![c],
            Budget {
                entries: 10,
                chars: 4,
            },
        );
        assert!(over.over_budget);
        assert_eq!(over.omitted, 1);
    }

    #[test]
    fn pipeline_budget_counts_pre_escape_chars() {
        // "a<b>" 转义前 4 字、转义后 10 字：chars=4 放行 ⇒ 预算按转义前原文计数（§5.2-5）
        let c = cand("a.md", 1, "T", "a<b>", false, "person", "");
        let out = pipeline(
            vec![c],
            Budget {
                entries: 10,
                chars: 4,
            },
        );
        assert_eq!(contents(&out), vec!["a<b>"]);
        assert!(!out.over_budget);
    }
}
