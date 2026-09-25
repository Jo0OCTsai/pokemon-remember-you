//! inbox 集成测试（C4）：顶层视图（count_today/today_filenames/pending）、
//! staging 读写与 promote_move、解析失败文件跳过、legacy_bootstrap。
//!
//! 依赖 B3 的 parse_proposal_file / parse_filename_date_source（dex-core）。

use dex_core::guard::InboxView;
use dex_store::inbox::Inbox;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// 合法提案文件（frontmatter + 正文；与 proposal::render_proposal_file 格式一致）
fn proposal(source: &str, kind: &str, evidence: &str, content: &str) -> String {
    format!("---\nsource: {source}\nkind: {kind}\nevidence: {evidence}\n---\n{content}\n")
}

const DATE: &str = "2026-09-25";

fn fixture() -> TempDir {
    let root = TempDir::new().unwrap();
    // 顶层提案：claude 今日 2 条、pi 今日 1 条、claude 昨日 1 条、非提案命名 1 条
    write(
        root.path(),
        "inbox/2026-09-25-claude-aaaaaa.md",
        &proposal("claude", "fact", "chat.md#L1", "claude 今日 1"),
    );
    write(
        root.path(),
        "inbox/2026-09-25-claude-bbbbbb.md",
        &proposal("claude", "preference", "chat.md#L2", "claude 今日 2"),
    );
    write(
        root.path(),
        "inbox/2026-09-25-pi-cccccc.md",
        &proposal("pi", "pattern", "note.md#L3", "pi 今日 1"),
    );
    write(
        root.path(),
        "inbox/2026-09-24-claude-dddddd.md",
        &proposal("claude", "fact", "chat.md#L4", "claude 昨日"),
    );
    write(root.path(), "inbox/scratch-note.md", "随手记，非提案命名\n");
    // staging / bootstrap 子目录（顶层视图须排除）
    write(
        root.path(),
        "inbox/staging/claude/c001.md",
        &proposal("claude", "fact", "e", "staged"),
    );
    write(
        root.path(),
        "inbox/bootstrap/b1.md",
        &proposal("claude", "fact", "e", "v0 遗留"),
    );
    write(
        root.path(),
        "inbox/bootstrap/b2.md",
        &proposal("claude", "fact", "e", "v0 遗留 2"),
    );
    root
}

#[test]
fn list_top_level_only_files_md_sorted() {
    let root = fixture();
    let inbox = Inbox::new(root.path());
    assert_eq!(
        inbox.list_top_level(),
        vec![
            "2026-09-24-claude-dddddd.md".to_string(),
            "2026-09-25-claude-aaaaaa.md".to_string(),
            "2026-09-25-claude-bbbbbb.md".to_string(),
            "2026-09-25-pi-cccccc.md".to_string(),
            "scratch-note.md".to_string(),
        ]
    );
}

#[test]
fn count_and_filenames_by_source_date() {
    let root = fixture();
    let inbox = Inbox::new(root.path());
    assert_eq!(inbox.count_today("claude", DATE).unwrap(), 2);
    assert_eq!(
        inbox.today_filenames("claude", DATE).unwrap(),
        vec![
            "2026-09-25-claude-aaaaaa.md".to_string(),
            "2026-09-25-claude-bbbbbb.md".to_string()
        ]
    );
    assert_eq!(inbox.count_today("pi", DATE).unwrap(), 1);
    // 昨日不计入今日
    assert_eq!(inbox.count_today("claude", "2026-09-24").unwrap(), 1);
    // 未知 source / 非提案命名不计
    assert_eq!(inbox.count_today("nobody", DATE).unwrap(), 0);
}

#[test]
fn pending_parses_top_level_and_skips_bad_files() {
    let root = fixture();
    // 坏文件：frontmatter 损坏 → pending 跳过
    write(
        root.path(),
        "inbox/2026-09-25-claude-eeeeee.md",
        "---\nnot yaml at all\n",
    );
    let inbox = Inbox::new(root.path());

    let mut pending = inbox.pending().unwrap();
    pending.sort_by(|a, b| a.path.cmp(&b.path));
    // scratch-note 非提案命名但在顶层 *.md——解析失败 → 跳过；
    // staging/bootstrap 不在 pending（只读顶层）
    assert_eq!(
        pending.len(),
        4,
        "4 条可解析顶层提案（含昨日）；坏文件与随手记跳过"
    );
    let sources: Vec<(String, String)> = pending
        .iter()
        .map(|p| (p.source.clone(), p.kind.clone()))
        .collect();
    assert!(sources.contains(&("claude".into(), "fact".into())));
    assert!(sources.contains(&("claude".into(), "preference".into())));
    assert!(sources.contains(&("pi".into(), "pattern".into())));
    // 正文解析往返（trim_end 容忍 B3 对尾部换行的往返口径）
    let first = pending.iter().find(|p| p.source == "pi").unwrap();
    assert!(
        first.content.trim_end() == "pi 今日 1",
        "实际：{:?}",
        first.content
    );
    assert_eq!(
        first.path,
        root.path().join("inbox/2026-09-25-pi-cccccc.md")
    );
}

#[test]
fn staging_write_list_and_promote() {
    let root = TempDir::new().unwrap();
    let inbox = Inbox::new(root.path());

    // staging 目录派生
    assert_eq!(
        inbox.staging_dir("claude"),
        root.path().join("inbox/staging/claude")
    );

    // stage_write：目录不存在则建；返回落盘路径
    let p1 = inbox
        .stage_write("claude", "c001", &proposal("claude", "fact", "e", "候选 1"))
        .unwrap();
    let p2 = inbox
        .stage_write("claude", "c002", &proposal("claude", "fact", "e", "候选 2"))
        .unwrap();
    assert_eq!(p1, root.path().join("inbox/staging/claude/c001.md"));
    assert!(p1.exists() && p2.exists());

    // list_staging：排序
    assert_eq!(inbox.list_staging("claude"), vec![p1.clone(), p2.clone()]);
    assert!(inbox.list_staging("nobody").is_empty());

    // promote_move：staging → inbox 顶层（重命名标准命名式）
    let promoted = inbox
        .promote_move("claude", "c001.md", "2026-09-25-claude-abc123.md")
        .unwrap();
    assert_eq!(
        promoted,
        root.path().join("inbox/2026-09-25-claude-abc123.md")
    );
    assert!(promoted.exists(), "目标存在");
    assert!(!p1.exists(), "源已移动");
    assert_eq!(inbox.list_staging("claude"), vec![p2]);

    // 源不存在 → E_NOT_FOUND
    let err = inbox
        .promote_move("claude", "c001.md", "again.md")
        .unwrap_err();
    assert_eq!(err.code(), "E_NOT_FOUND");
}

#[test]
fn legacy_bootstrap_counts_only_when_present() {
    let root = fixture();
    let inbox = Inbox::new(root.path());
    assert_eq!(inbox.legacy_bootstrap(), Some(2));

    let bare = TempDir::new().unwrap();
    fs::create_dir_all(bare.path().join("inbox")).unwrap();
    assert_eq!(Inbox::new(bare.path()).legacy_bootstrap(), None);
}

#[test]
fn views_on_missing_inbox_dir_are_empty() {
    let root = TempDir::new().unwrap();
    let inbox = Inbox::new(root.path());
    assert!(inbox.list_top_level().is_empty());
    assert_eq!(inbox.count_today("claude", DATE).unwrap(), 0);
    assert!(inbox.today_filenames("claude", DATE).unwrap().is_empty());
    assert!(inbox.pending().unwrap().is_empty());
    assert_eq!(inbox.legacy_bootstrap(), None);
}
