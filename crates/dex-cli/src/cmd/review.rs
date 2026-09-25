//! `dex review`（FR-6.6 / §6.3）：周回顾七段清单（权威段序，冻结）：
//!
//! ```text
//! ① lint 体检（FR-6.11 检查集）      ② inbox 待裁决提案（来源/证据/置信度）
//! ③ journal 提升候选（本周每日页）   ④ 衰减清单＋Spoke 使用周报粘贴区
//! ⑤ scope 升降级候选                 ⑥ 近义预筛与矛盾组（运行时相似度聚类，仅聚类不裁决）
//! ⑦ 结构整理候选（lint 结构面）
//! ```
//!
//! 冻结语义：
//! - 每项附建议命令（FR-9.2：git mv/rm 提示等）；清单按一级域/scope 前缀分组（FR-9.5）；
//!   `--group <一级域前缀>` 仅输出单组
//! - `--week N`：journal 回看窗口天数（缺省 7）
//! - inbox 非空 ⇒ 显式警告（周清空约束 FR-4.4）；退出码恒 0
//! - 段⑥ 确定性聚类（不落 index/）：同 topic 跨文件 → 矛盾组（superseded-by 建议）；
//!   归一化 token 相似度（如 bigram Jaccard ≥ 0.6）→ 近义组
//! - 段⑤ 确定性候选：同一 H1 topic 出现在 ≥2 个 apps/* scope → 升级候选；否则空
//!
//! 实现注（D 组）：
//! - 段① 复用 [`crate::cmd::lint::run_checks`]、段④ 复用 [`crate::cmd::stale::collect_stale`]
//!   （勿重复实现）；段⑦ = ① 的结构面子集（按 check 名过滤）
//! - 段⑥ 相似度：归一化（仅字母数字 + 小写）字符 bigram 的 Jaccard ≥ 0.6，
//!   同 scope 树内两两判定后并查集成组（确定性输出：按 path/line 序）
//! - inbox_pending 与 FR-4.4 警告为全局计数（不受 `--group` 过滤影响）

use crate::cmd::lint::{run_checks, text_finding_line, Finding};
use crate::cmd::stale::{collect_stale, four_layer_entries};
use crate::cmd::CommonOpts;
use crate::guard_runtime::{today_local, Ctx};
use chrono::NaiveDate;
use clap::Args;
use dex_core::DexError;
use dex_core::Warning;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// review 段⑦ 结构面检查（FR-6.6：FR-2.8/2.9 结构面）
const STRUCTURE_CHECKS: [&str; 9] = [
    "top-level-whitelist",
    "dir-naming",
    "domains-depth",
    "apps-projects-depth",
    "empty-content-dir",
    "domains-budget",
    "file-soft-limit",
    "archive-mirror",
    "config-scope-dangling",
];
/// 段④ 占位说明（引用豁免由人按周报执行——v1 无协议通道）
const SPOKE_NOTE: &str = "Spoke 使用周报粘贴区：引用豁免由人按周报执行（v1 无协议通道）";
/// 一级域分组输出序（FR-9.5；其余前缀按字典序追加）
const GROUP_ORDER: [&str; 6] = ["person", "domains", "apps", "projects", "journal", "inbox"];
/// 近义组 bigram Jaccard 阈值
const SIMILARITY_THRESHOLD: f64 = 0.6;

#[derive(Args, Debug)]
pub struct ReviewArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// journal 回看窗口天数（缺省 7）
    #[arg(long)]
    pub week: Option<u32>,
    /// 仅输出该一级域/scope 前缀分组（如 domains、projects/foo）
    #[arg(long)]
    pub group: Option<String>,
}

struct InboxItem {
    path: String,
    source: String,
    kind: String,
    confidence: i64,
    evidence: String,
    content: String,
}

struct JournalPage {
    path: String,
    items: Vec<(usize, String)>,
}

struct ScopeMove {
    topic: String,
    scopes: Vec<String>,
    paths: Vec<String>,
}

struct Contradiction {
    topic: String,
    files: Vec<(String, usize)>,
}

struct SynonymGroup {
    scope: String,
    members: Vec<(String, usize, String)>,
}

pub fn run(args: &ReviewArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let root = &ctx.root;
    let week = args.week.unwrap_or(7).max(1);
    let today = today_local();

    // ① lint 体检（全量 findings）
    let mut findings = run_checks(root, &ctx.config);

    // ② inbox 待裁决提案（frontmatter 解析失败的文件由 lint 报告，此处跳过）
    let mut inbox_items = collect_inbox_items(root);
    let inbox_pending = inbox_items.len();

    // ③ journal 提升候选（近 week 天每日页列表项）
    let mut journal_pages = collect_journal(root, &today, week);

    // ④ 衰减清单（引用豁免 = Spoke 周报粘贴区人工步骤，v1 无协议通道）
    let (mut candidates, stale_warnings) = collect_stale(root, &ctx.config, None);

    // ⑤⑥ 的输入：四层条目（确定性：walk 字典序 + 行序）
    let entries = four_layer_entries(root);
    let mut scope_moves = compute_scope_moves(&entries);
    let mut contradictions = compute_contradictions(&entries);
    let mut synonym_groups = compute_synonym_groups(&entries);

    // --group：段级 item 按路径段前缀过滤（全局计数 inbox_pending 不受影响）
    if let Some(g) = args.group.as_deref() {
        let keep = |p: &str| path_matches(p, g);
        findings.retain(|f| f.path.as_deref().is_some_and(&keep));
        inbox_items.retain(|i| keep(&i.path));
        journal_pages.retain(|p| keep(&p.path));
        candidates.retain(|c| keep(&c.path));
        scope_moves.retain(|m| m.paths.iter().any(|p| keep(p)));
        for m in &mut scope_moves {
            m.paths.retain(|p| keep(p));
        }
        for c in &mut contradictions {
            c.files.retain(|(p, _)| keep(p));
        }
        contradictions.retain(|c| c.files.len() >= 2);
        for s in &mut synonym_groups {
            s.members.retain(|(p, _, _)| keep(p));
        }
        synonym_groups.retain(|s| s.members.len() >= 2);
    }

    // ⑦ 结构整理候选（① 的结构面子集）
    let structure: Vec<Finding> = findings
        .iter()
        .filter(|f| STRUCTURE_CHECKS.contains(&f.check))
        .cloned()
        .collect();

    // 分组（FR-9.5）：条目按一级域前缀计数
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let bump = |path: &str, counts: &mut BTreeMap<String, usize>| {
        let g = path.split('/').next().unwrap_or(path).to_string();
        *counts.entry(g).or_insert(0) += 1;
    };
    for f in &findings {
        if let Some(p) = &f.path {
            bump(p, &mut counts);
        }
    }
    for i in &inbox_items {
        bump(&i.path, &mut counts);
    }
    for p in &journal_pages {
        bump(&p.path, &mut counts);
    }
    for c in &candidates {
        bump(&c.path, &mut counts);
    }
    for m in &scope_moves {
        for p in &m.paths {
            bump(p, &mut counts);
        }
    }
    for c in &contradictions {
        for (p, _) in &c.files {
            bump(p, &mut counts);
        }
    }
    for s in &synonym_groups {
        for (p, _, _) in &s.members {
            bump(p, &mut counts);
        }
    }
    let mut groups_json = Vec::new();
    for g in GROUP_ORDER {
        if let Some(n) = counts.get(g) {
            groups_json.push(json!({ "group": g, "count": n }));
        }
    }
    for (g, n) in &counts {
        if !GROUP_ORDER.contains(&g.as_str()) {
            groups_json.push(json!({ "group": g, "count": n }));
        }
    }

    // json 装配（sections 键序 = 段序；serde_json 字典序输出稳定）
    let inbox_json: Vec<_> = inbox_items
        .iter()
        .map(|i| {
            json!({
                "path": i.path, "source": i.source, "kind": i.kind,
                "confidence": i.confidence, "evidence": i.evidence, "content": i.content,
            })
        })
        .collect();
    let journal_json: Vec<_> = journal_pages
        .iter()
        .map(|p| {
            json!({
                "path": p.path,
                "items": p.items.iter().map(|(l, c)| json!({ "line": l, "content": c })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let moves_json: Vec<_> = scope_moves
        .iter()
        .map(|m| {
            json!({
                "topic": m.topic, "scopes": m.scopes, "paths": m.paths,
                "target": "domains/person",
            })
        })
        .collect();
    let contra_json: Vec<_> = contradictions
        .iter()
        .map(|c| {
            json!({
                "topic": c.topic,
                "files": c.files.iter().map(|(p, l)| json!({ "path": p, "line": l })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let syn_json: Vec<_> = synonym_groups
        .iter()
        .map(|s| {
            json!({
                "scope": s.scope,
                "members": s.members.iter()
                    .map(|(p, l, c)| json!({ "path": p, "line": l, "content": c }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let data = json!({
        "sections": {
            "lint": &findings,
            "inbox": inbox_json,
            "journal": journal_json,
            "stale": { "candidates": &candidates, "note": SPOKE_NOTE },
            "scope_moves": moves_json,
            "near_synonyms": { "contradictions": contra_json, "synonyms": syn_json },
            "structure": &structure,
        },
        "inbox_pending": inbox_pending,
        "groups": groups_json,
    });

    // text 装配（段序 = FR-6.6 权威序）
    let mut text = String::new();
    text.push_str(&format!("dex review 周回顾清单（近 {week} 天）\n\n"));

    text.push_str("## ① lint 体检\n");
    if findings.is_empty() {
        text.push_str("（无）\n");
    } else {
        for f in &findings {
            text.push_str(&text_finding_line(f));
            text.push('\n');
        }
    }
    text.push_str("建议命令：dex lint\n\n");

    text.push_str(&format!("## ② inbox 待裁决提案（{inbox_pending}）\n"));
    if inbox_items.is_empty() {
        text.push_str("（无）\n");
    }
    for i in &inbox_items {
        text.push_str(&format!(
            "- {} | source={} kind={} confidence={} evidence={}\n",
            i.path, i.source, i.kind, i.confidence, i.evidence
        ));
        text.push_str(&format!(
            "  确认：git mv {} <scope>/<path>（剥 frontmatter）｜否决：rm {}\n",
            i.path, i.path
        ));
    }
    text.push('\n');

    text.push_str(&format!("## ③ journal 提升候选（近 {week} 天）\n"));
    if journal_pages.is_empty() {
        text.push_str("（无）\n");
    }
    for p in &journal_pages {
        for (line, content) in &p.items {
            text.push_str(&format!("- {}:{line} {content}\n", p.path));
        }
        text.push_str(&format!("  建议命令：dex read {}\n", p.path));
    }
    text.push('\n');

    text.push_str(&format!("## ④ 衰减清单（{}）\n", candidates.len()));
    if candidates.is_empty() {
        text.push_str("（无）\n");
    }
    for c in &candidates {
        text.push_str(&format!(
            "- {}:{} | {} | 最后变更 {} | 建议 {}{}\n",
            c.path,
            c.line,
            c.scope,
            c.last_change,
            suggestion_str(c),
            if c.force_review { " !复审" } else { "" }
        ));
    }
    text.push_str(&format!("（{SPOKE_NOTE}）\n\n"));

    text.push_str(&format!("## ⑤ scope 升降级候选（{}）\n", scope_moves.len()));
    if scope_moves.is_empty() {
        text.push_str("（无）\n");
    }
    for m in &scope_moves {
        text.push_str(&format!(
            "- topic「{}」出现于 {} → 升级候选：迁至 domains/ 或 person/\n",
            m.topic,
            m.scopes.join(", ")
        ));
    }
    text.push('\n');

    text.push_str("## ⑥ 近义与矛盾组\n");
    text.push_str("矛盾组（同 topic 跨不同文件，建议 superseded-by 显式化，FR-2.5）：\n");
    if contradictions.is_empty() {
        text.push_str("（无）\n");
    }
    for c in &contradictions {
        let locs: Vec<String> = c.files.iter().map(|(p, l)| format!("{p}:{l}")).collect();
        text.push_str(&format!("- topic「{}」：{}\n", c.topic, locs.join(" ↔ ")));
    }
    text.push_str("近义组（同 scope 树内 bigram Jaccard ≥ 0.6，仅聚类不裁决）：\n");
    if synonym_groups.is_empty() {
        text.push_str("（无）\n");
    }
    for s in &synonym_groups {
        let locs: Vec<String> = s
            .members
            .iter()
            .map(|(p, l, _)| format!("{p}:{l}"))
            .collect();
        text.push_str(&format!("- [{}] {}\n", s.scope, locs.join(" ≈ ")));
    }
    text.push('\n');

    text.push_str(&format!("## ⑦ 结构整理候选（{}）\n", structure.len()));
    if structure.is_empty() {
        text.push_str("（无）\n");
    } else {
        for f in &structure {
            text.push_str(&text_finding_line(f));
            text.push('\n');
        }
    }

    // inbox 非空 → 显式警告（FR-4.4 周清空约束）：json 进 warnings[]，text 尾部提示
    let mut extra = stale_warnings;
    if inbox_pending > 0 {
        let w = Warning::ConfigNotice {
            detail: "inbox 未清空（FR-4.4 周清空约束）——分批直至清空".into(),
        };
        if ctx.json {
            extra.push(w);
        } else {
            text.push_str("\n⚠ inbox 未清空（FR-4.4 周清空约束）——分批直至清空\n");
        }
    }

    Ok(ctx.emit(0, extra, data, text))
}

// ---------- 段②③ ----------

fn collect_inbox_items(root: &Path) -> Vec<InboxItem> {
    let inbox = dex_store::inbox::Inbox::new(root);
    let mut out = Vec::new();
    for name in inbox.list_top_level() {
        let rel = format!("inbox/{name}");
        let Ok(raw) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        // 解析失败文件跳过（坏文件由 lint 报告，冻结契约）
        let Ok((meta, content)) = dex_core::proposal::parse_proposal_file(&raw) else {
            continue;
        };
        out.push(InboxItem {
            path: rel,
            source: meta.source,
            kind: meta.kind,
            confidence: meta
                .confidence
                .unwrap_or(dex_core::proposal::DEFAULT_CONFIDENCE),
            evidence: meta.evidence,
            content,
        });
    }
    out
}

fn collect_journal(root: &Path, today: &str, week: u32) -> Vec<JournalPage> {
    let Ok(t) = NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return Vec::new();
    };
    let mut pages = Vec::new();
    for k in (0..week).rev() {
        let date = (t - chrono::Duration::days(i64::from(k)))
            .format("%Y-%m-%d")
            .to_string();
        let rel = format!("journal/{date}.md");
        let Ok(content) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        // 列表项才是提升候选素材（段落条目不列）
        let items: Vec<(usize, String)> = dex_core::entry::parse_file(&rel, &content)
            .into_iter()
            .filter(|e| ["- ", "* ", "+ "].iter().any(|m| e.content.starts_with(m)))
            .map(|e| (e.line, e.content))
            .collect();
        if !items.is_empty() {
            pages.push(JournalPage { path: rel, items });
        }
    }
    pages
}

// ---------- 段⑤⑥（确定性计算） ----------

/// 段⑤：同一 H1 topic 出现在 ≥2 个 apps/* scope → 升级候选（apps→domains/person）
fn compute_scope_moves(entries: &[dex_core::entry::Entry]) -> Vec<ScopeMove> {
    let mut topics: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for e in entries {
        if e.topic.is_empty() || !e.path.starts_with("apps/") {
            continue;
        }
        if let Some(scope) = scope_of(&e.path) {
            let entry = topics.entry(e.topic.clone()).or_default();
            entry.0.insert(scope);
            entry.1.insert(e.path.clone());
        }
    }
    topics
        .into_iter()
        .filter(|(_, (scopes, _))| scopes.len() >= 2)
        .map(|(topic, (scopes, paths))| ScopeMove {
            topic,
            scopes: scopes.into_iter().collect(),
            paths: paths.into_iter().collect(),
        })
        .collect()
}

/// 段⑥ 矛盾组：同 topic 跨不同文件（每文件取首条条目行）
fn compute_contradictions(entries: &[dex_core::entry::Entry]) -> Vec<Contradiction> {
    let mut topics: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for e in entries {
        if e.topic.is_empty() {
            continue;
        }
        let files = topics.entry(e.topic.clone()).or_default();
        files
            .entry(e.path.clone())
            .and_modify(|l| *l = (*l).min(e.line))
            .or_insert(e.line);
    }
    topics
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(topic, files)| Contradiction {
            topic,
            files: files.into_iter().collect(),
        })
        .collect()
}

/// 段⑥ 近义组：同 scope 树内归一化字符 bigram Jaccard ≥ 0.6（并查集成组，确定性）
fn compute_synonym_groups(entries: &[dex_core::entry::Entry]) -> Vec<SynonymGroup> {
    let mut by_scope: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        if let Some(scope) = scope_of(&e.path) {
            by_scope.entry(scope).or_default().push(i);
        }
    }
    let mut parent: Vec<usize> = (0..entries.len()).collect();
    for idxs in by_scope.values() {
        for (a, &i) in idxs.iter().enumerate() {
            for &j in &idxs[a + 1..] {
                if jaccard(&entries[i].content, &entries[j].content) >= SIMILARITY_THRESHOLD {
                    union(&mut parent, i, j);
                }
            }
        }
    }
    let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, _) in entries.iter().enumerate() {
        let r = find(&mut parent, i);
        by_root.entry(r).or_default().push(i);
    }
    by_root
        .into_values()
        .filter(|members| members.len() >= 2)
        .map(|members| SynonymGroup {
            scope: scope_of(&entries[members[0]].path).unwrap_or_default(),
            members: members
                .into_iter()
                .map(|i| {
                    (
                        entries[i].path.clone(),
                        entries[i].line,
                        entries[i].content.clone(),
                    )
                })
                .collect(),
        })
        .collect()
}

fn scope_of(path: &str) -> Option<String> {
    dex_core::scope::scope_of_path(Path::new(path)).map(|s| s.as_str().to_string())
}

fn suggestion_str(c: &dex_core::decay::StaleCandidate) -> &'static str {
    match dex_core::decay::suggestion_for(c) {
        dex_core::decay::Suggestion::Archive => "archive",
        dex_core::decay::Suggestion::Rewrite => "rewrite",
        dex_core::decay::Suggestion::Keep => "keep",
    }
}

// ---------- 并查集 + 相似度 ----------

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[rb] = ra;
    }
}

/// 归一化（小写、去标点——仅保留字母数字）后的字符 bigram 集合
fn normalized_bigrams(s: &str) -> BTreeSet<(char, char)> {
    let norm: Vec<char> = s
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    norm.windows(2).map(|w| (w[0], w[1])).collect()
}

fn jaccard(a: &str, b: &str) -> f64 {
    let (ba, bb) = (normalized_bigrams(a), normalized_bigrams(b));
    if ba.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let inter = ba.intersection(&bb).count();
    let uni = ba.union(&bb).count();
    inter as f64 / uni as f64
}

/// 段级前缀匹配：`domains/work` 匹配 `domains/work/x.md`，不匹配 `domains/workx`
fn path_matches(path: &str, prefix: &str) -> bool {
    let p: Vec<&str> = path.split('/').collect();
    let s: Vec<&str> = prefix.split('/').filter(|x| !x.is_empty()).collect();
    !s.is_empty() && s.len() <= p.len() && s.iter().zip(&p).all(|(a, b)| a == b)
}
