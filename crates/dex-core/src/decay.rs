//! §5.4 衰减判定（纯函数）：单遍快照 → 候选集、keep-until 两级豁免、到期强制重列、周报引用豁免。
//!
//! 冻结语义（FR-6.5/2.10，DESIGN §5.4）：
//! - 候选 = `now − last_substantive(文件) ≥ days(scope)`（v1 单遍口径：触及即变更，
//!   不做 diff 级过滤；分档键 = scope 前缀最长匹配）
//! - keep-until（条目级优先于文件级——entry.keep_until 已是生效值）：未到期 → 豁免不列示；
//!   **已到期 → 重新列入且 force_review=true（强制复审，豁免永远不是永久决定）**
//! - 周报引用豁免：`referenced` 含 `"path:line"` 键 → 豁免
//! - 无快照日期（未跟踪新文件等）→ 跳过（保守近似，避免误报；口径记录于实现注）
//! - 建议：keep-until 已到期 → 建议复审（KeepUntil 动作）；否则 Archive/Rewrite 二选一提示
//!
//! **B1 任务（TDD）**：先写失败测试再实现；公共 API 冻结。

use crate::entry::{Entry, KeepUntil};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// 衰减分档策略（§2.5 `[stale]`；per-scope 前缀最长匹配覆盖缺省）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StalePolicy {
    /// 缺省窗口天数（90）
    pub default_days: i64,
    /// `[stale.scopes."<前缀>"].days`
    pub per_scope: Vec<(String, i64)>,
}

/// scope 前缀最长匹配 → 覆盖天数；无匹配 → 缺省。
/// 前缀按路径段比较：`domains/work` 匹配 `domains/work` 与 `domains/work/x`，
/// 不匹配 `domains/workx`；多个前缀命中时取段数最长者。
pub fn days_for(scope: &str, policy: &StalePolicy) -> i64 {
    let segs: Vec<&str> = scope.split('/').filter(|s| !s.is_empty()).collect();
    let mut best: Option<(usize, i64)> = None;
    for (prefix, days) in &policy.per_scope {
        let p: Vec<&str> = prefix.split('/').filter(|s| !s.is_empty()).collect();
        if p.len() <= segs.len() && p.iter().zip(&segs).all(|(a, b)| a == b) {
            let longer = match &best {
                None => true,
                Some((len, _)) => p.len() > *len,
            };
            if longer {
                best = Some((p.len(), *days));
            }
        }
    }
    match best {
        Some((_, days)) => days,
        None => policy.default_days,
    }
}

/// 衰减候选（stale 清单条目）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleCandidate {
    pub path: String,
    pub line: usize,
    pub topic: String,
    /// 条目摘录（首 80 字符）
    pub excerpt: String,
    /// 条目所属 scope（`domains/work` 等）
    pub scope: String,
    /// 文件最后实质变更日（v1 = 单遍 git log 快照；git 缺失回退 mtime 由调用方注入）
    pub last_change: String,
    /// keep-until 已到期 → 强制复审（FR-2.10）
    pub force_review: bool,
    /// 生效 keep-until（过期时仍带出供复审参考）
    pub keep_until: Option<KeepUntil>,
}

/// 建议动作（stale 输出「建议」，US-06 人裁决三选一）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Suggestion {
    /// git mv → archive/
    Archive,
    /// 更新原文（更新即续命）
    Rewrite,
    /// 保留：加 keep-until 注释 / 到期条目复审
    Keep,
}

/// 候选建议：force_review 或无 keep-until → Keep（复审/保留裁决）；否则 Archive/Rewrite 并列提示由人决。
/// 冻结规则：`force_review == true` → Keep（到期复审即「保留还是归档」再裁决）；
/// 否则 → Archive（清单默认建议，人可改写/保留）。
pub fn suggestion_for(candidate: &StaleCandidate) -> Suggestion {
    if candidate.force_review {
        Suggestion::Keep
    } else {
        Suggestion::Archive
    }
}

/// 候选集计算（纯函数）。
/// - `entries`：四层 scope 内解析出的全部条目（含 keep_until 生效值）
/// - `last_change`：文件路径 → 日期（v1 单遍 git log 快照 / mtime 回退，由调用方注入）
/// - `today`：`YYYY-MM-DD`
/// - `referenced`：周报引用豁免键 `"path:line"`
pub fn compute_candidates(
    entries: &[Entry],
    scope_of: &dyn Fn(&str) -> Option<String>,
    last_change: &dyn Fn(&str) -> Option<String>,
    today: &str,
    policy: &StalePolicy,
    referenced: &[String],
) -> Vec<StaleCandidate> {
    // today 非法：一切日期比较均无从谈起，保守返回空清单（格式问题由 lint 报）。
    let Ok(today) = NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for e in entries {
        // 周报引用豁免（最终覆盖：依据 §5.4「到期重列后叠加引用豁免」的顺序，引用豁免胜出）
        if referenced
            .iter()
            .any(|r| r == &format!("{}:{}", e.path, e.line))
        {
            continue;
        }
        // 无 scope（不在四层内）→ 跳过
        let Some(scope) = scope_of(&e.path) else {
            continue;
        };
        // 无快照（未跟踪新文件等）→ 跳过（保守近似，避免误报）
        let Some(last_str) = last_change(&e.path) else {
            continue;
        };
        let Ok(last) = NaiveDate::parse_from_str(&last_str, "%Y-%m-%d") else {
            continue;
        };

        // keep-until（entry.keep_until 已是两级合并后的生效值）
        let mut force_review = false;
        if let Some(ku) = &e.keep_until {
            match NaiveDate::parse_from_str(&ku.until, "%Y-%m-%d") {
                Ok(until) if until >= today => continue, // 未到期 → 豁免不列示
                _ => force_review = true, // 已到期（或日期非法）→ 无论年龄列入，强制复审
            }
        }

        // 年龄候选：now − last_change ≥ days(scope)
        if !force_review && (today - last).num_days() < days_for(&scope, policy) {
            continue;
        }

        out.push(StaleCandidate {
            path: e.path.clone(),
            line: e.line,
            topic: e.topic.clone(),
            excerpt: e.content.chars().take(80).collect(),
            scope,
            last_change: last_str,
            force_review,
            keep_until: e.keep_until.clone(),
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-09-25";

    fn entry(path: &str, line: usize, content: &str) -> Entry {
        Entry {
            path: path.to_string(),
            line,
            topic: "T".to_string(),
            content: content.to_string(),
            handwritten: true,
            superseded_by: None,
            keep_until: None,
        }
    }

    fn keep(until: &str) -> Option<KeepUntil> {
        Some(KeepUntil {
            until: until.to_string(),
            reason: "r".to_string(),
        })
    }

    fn policy(default: i64, per_scope: &[(&str, i64)]) -> StalePolicy {
        StalePolicy {
            default_days: default,
            per_scope: per_scope.iter().map(|(s, d)| (s.to_string(), *d)).collect(),
        }
    }

    fn always_scope(scope: &str) -> impl Fn(&str) -> Option<String> {
        let scope = scope.to_string();
        move |_p: &str| Some(scope.clone())
    }

    // ---------- days_for ----------

    #[test]
    fn days_for_longest_segment_prefix() {
        let p = policy(90, &[("domains", 30), ("domains/work", 45)]);
        assert_eq!(days_for("domains/work", &p), 45);
        assert_eq!(days_for("domains/work/deep", &p), 45);
        assert_eq!(days_for("domains", &p), 30);
        assert_eq!(days_for("domains/workx", &p), 30); // 段级比较：不误配 workx
        assert_eq!(days_for("person", &p), 90); // 无匹配 → default
        assert_eq!(days_for("domains/work/very/deep", &p), 45); // 最长前缀胜出
    }

    #[test]
    fn days_for_empty_per_scope_returns_default() {
        let p = policy(120, &[]);
        assert_eq!(days_for("anything/else", &p), 120);
    }

    // ---------- compute_candidates：年龄边界与分档 ----------

    #[test]
    fn compute_candidates_age_boundary_inclusive() {
        let es = vec![entry("old.md", 2, "- x"), entry("new.md", 2, "- y")];
        let scope = always_scope("projects/foo");
        // old.md = 恰好 90 天前；new.md = 89 天前
        let last = |p: &str| {
            Some(if p == "old.md" {
                "2026-06-27".to_string()
            } else {
                "2026-06-28".to_string()
            })
        };
        let got = compute_candidates(&es, &scope, &last, TODAY, &policy(90, &[]), &[]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "old.md"); // ≥90 列入
        assert!(!got[0].force_review);
        assert_eq!(got[0].excerpt, "- x");
        assert_eq!(got[0].last_change, "2026-06-27");
        assert_eq!(got[0].scope, "projects/foo");
    }

    #[test]
    fn compute_candidates_per_scope_windows() {
        let es = vec![
            entry("w.md", 1, "- w"), // domains/work → 45 天
            entry("d.md", 1, "- d"), // domains → 30 天
            entry("p.md", 1, "- p"), // person → 90 天
        ];
        let scope = |p: &str| {
            Some(
                if p == "w.md" {
                    "domains/work"
                } else if p == "d.md" {
                    "domains"
                } else {
                    "person"
                }
                .to_string(),
            )
        };
        let last = |_p: &str| Some("2026-08-16".to_string()); // 全部 40 天前
        let pol = policy(90, &[("domains", 30), ("domains/work", 45)]);
        let got = compute_candidates(&es, &scope, &last, TODAY, &pol, &[]);
        let paths: Vec<&str> = got.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, vec!["d.md"]); // 40≥30 ✓；40<45（work 走 45 档而非 30）、40<90 ✗

        // w.md 满 45 天后进入（证明其分档是 45 而非 30/90）
        let last45 = |p: &str| {
            Some(if p == "w.md" {
                "2026-08-11".to_string() // 恰 45 天前
            } else {
                "2026-08-16".to_string()
            })
        };
        let got = compute_candidates(&es, &scope, &last45, TODAY, &pol, &[]);
        let paths: Vec<&str> = got.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, vec!["d.md", "w.md"]); // 45≥45 ✓、40≥30 ✓
    }

    // ---------- compute_candidates：keep-until ----------

    #[test]
    fn compute_candidates_keep_until_unexpired_exempt_even_when_old() {
        let mut e = entry("a.md", 2, "- kept");
        e.keep_until = keep("2026-12-31");
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2020-01-01".to_string());
        let got = compute_candidates(&[e], &scope, &last, TODAY, &policy(90, &[]), &[]);
        assert!(got.is_empty());
    }

    #[test]
    fn compute_candidates_keep_until_expired_forces_review_regardless_of_age() {
        let mut young_expired = entry("y.md", 2, "- young");
        young_expired.keep_until = keep("2026-01-01"); // 已到期
        let mut same_day = entry("s.md", 2, "- same day");
        same_day.keep_until = keep(TODAY); // until == today → 仍未到期（>= today 豁免）
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2026-09-24".to_string()); // 1 天前 → 年轻
        let got = compute_candidates(
            &[young_expired, same_day],
            &scope,
            &last,
            TODAY,
            &policy(90, &[]),
            &[],
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "y.md");
        assert!(got[0].force_review);
        assert_eq!(got[0].keep_until.as_ref().unwrap().until, "2026-01-01");
        assert_eq!(suggestion_for(&got[0]), Suggestion::Keep);
    }

    #[test]
    fn compute_candidates_invalid_keep_until_date_treated_as_expired() {
        // 2026-02-30 形似但 NaiveDate 解析失败 → 视为已到期强制人审（lint 报格式）
        let mut e = entry("b.md", 2, "- bad date");
        e.keep_until = keep("2026-02-30");
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2026-09-24".to_string());
        let got = compute_candidates(&[e], &scope, &last, TODAY, &policy(90, &[]), &[]);
        assert_eq!(got.len(), 1);
        assert!(got[0].force_review);
    }

    // ---------- compute_candidates：引用豁免 / 无快照 ----------

    #[test]
    fn compute_candidates_referenced_exempt() {
        let es = vec![entry("a.md", 2, "- x"), entry("b.md", 3, "- y")];
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2020-01-01".to_string());
        let referenced = vec!["a.md:2".to_string()];
        let got = compute_candidates(&es, &scope, &last, TODAY, &policy(90, &[]), &referenced);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "b.md");
    }

    #[test]
    fn compute_candidates_referenced_beats_expired_keep_until() {
        // 决策（DESIGN §5.4 顺序：到期重列后叠加引用豁免）：referenced 豁免优先
        let mut e = entry("a.md", 2, "- x");
        e.keep_until = keep("2026-01-01");
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2020-01-01".to_string());
        let referenced = vec!["a.md:2".to_string()];
        let got = compute_candidates(&[e], &scope, &last, TODAY, &policy(90, &[]), &referenced);
        assert!(got.is_empty());
    }

    #[test]
    fn compute_candidates_no_snapshot_or_bad_snapshot_skips() {
        let es = vec![entry("a.md", 2, "- x")];
        let scope = always_scope("projects/foo");
        let none_last = |_p: &str| None::<String>;
        let got = compute_candidates(&es, &scope, &none_last, TODAY, &policy(90, &[]), &[]);
        assert!(got.is_empty());
        let bad_last = |_p: &str| Some("not-a-date".to_string());
        let got = compute_candidates(&es, &scope, &bad_last, TODAY, &policy(90, &[]), &[]);
        assert!(got.is_empty());
    }

    #[test]
    fn compute_candidates_missing_scope_skips_entry() {
        let es = vec![entry("a.md", 2, "- x")];
        let none_scope = |_p: &str| None::<String>;
        let last = |_p: &str| Some("2020-01-01".to_string());
        let got = compute_candidates(&es, &none_scope, &last, TODAY, &policy(90, &[]), &[]);
        assert!(got.is_empty());
    }

    #[test]
    fn compute_candidates_invalid_today_returns_empty() {
        let es = vec![entry("a.md", 2, "- x")];
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2020-01-01".to_string());
        let got = compute_candidates(&es, &scope, &last, "2026/09/25", &policy(90, &[]), &[]);
        assert!(got.is_empty());
    }

    // ---------- compute_candidates：排序 / 摘录 ----------

    #[test]
    fn compute_candidates_sorted_by_path_then_line() {
        let es = vec![
            entry("b.md", 5, "- b5"),
            entry("a.md", 9, "- a9"),
            entry("a.md", 2, "- a2"),
        ];
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2020-01-01".to_string());
        let got = compute_candidates(&es, &scope, &last, TODAY, &policy(90, &[]), &[]);
        let keys: Vec<(String, usize)> = got.iter().map(|c| (c.path.clone(), c.line)).collect();
        assert_eq!(
            keys,
            vec![
                ("a.md".to_string(), 2),
                ("a.md".to_string(), 9),
                ("b.md".to_string(), 5),
            ]
        );
    }

    #[test]
    fn compute_candidates_excerpt_truncates_80_unicode_chars() {
        let long = "记".repeat(100);
        let e = entry("a.md", 1, &long);
        let scope = always_scope("projects/foo");
        let last = |_p: &str| Some("2020-01-01".to_string());
        let got = compute_candidates(&[e], &scope, &last, TODAY, &policy(90, &[]), &[]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].excerpt.chars().count(), 80);
        assert!(got[0].excerpt.chars().all(|c| c == '记'));
    }

    // ---------- suggestion_for ----------

    #[test]
    fn suggestion_for_two_states() {
        let c = StaleCandidate {
            path: "a.md".to_string(),
            line: 2,
            topic: "T".to_string(),
            excerpt: "- x".to_string(),
            scope: "projects/foo".to_string(),
            last_change: "2026-06-27".to_string(),
            force_review: false,
            keep_until: None,
        };
        assert_eq!(suggestion_for(&c), Suggestion::Archive);
        let forced = StaleCandidate {
            force_review: true,
            ..c
        };
        assert_eq!(suggestion_for(&forced), Suggestion::Keep);
    }
}
