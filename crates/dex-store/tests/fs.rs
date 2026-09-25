//! fs 集成测试（C1）：.dex-ignore 生效、隐藏目录跳过、多前缀合并去重、不存在前缀跳过。

use dex_store::fs::{top_level_entries, walk_markdown};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn touch(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn dex_ignore_at_repo_root_is_applied_per_prefix() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/keep.md", "x");
    touch(root.path(), "person/secret.md", "x");
    touch(root.path(), "domains/coding/also-secret.md", "x");
    touch(root.path(), "domains/coding/keep.md", "x");
    // 仓库根 .dex-ignore：basename glob 模式（无斜杠 → 任意深度命中）
    fs::write(root.path().join(".dex-ignore"), "*secret.md\n").unwrap();

    let got = walk_markdown(
        root.path(),
        &[PathBuf::from("person"), PathBuf::from("domains/coding")],
    );
    assert_eq!(
        got,
        vec![
            "domains/coding/keep.md".to_string(),
            "person/keep.md".to_string()
        ]
    );
}

#[test]
fn dex_ignore_anchored_pattern_hits_subtree() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/a.md", "x");
    touch(root.path(), "person/private/b.md", "x");
    // 锚定模式（含斜杠 → 相对 .dex-ignore 所在目录）
    fs::write(root.path().join(".dex-ignore"), "person/private/\n").unwrap();

    let got = walk_markdown(root.path(), &[PathBuf::from("person")]);
    assert_eq!(got, vec!["person/a.md".to_string()]);
}

#[test]
fn hidden_entries_are_skipped() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/vis.md", "x");
    touch(root.path(), "person/.hidden/inv.md", "x");
    touch(root.path(), "person/.draft.md", "x");
    touch(root.path(), ".cache/cache.md", "x");
    touch(root.path(), ".git/HEAD.md", "x");

    let got = walk_markdown(root.path(), &[PathBuf::from("person")]);
    assert_eq!(got, vec!["person/vis.md".to_string()]);
}

#[test]
fn non_md_extensions_are_skipped() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/a.md", "x");
    touch(root.path(), "person/b.txt", "x");
    touch(root.path(), "person/c.markdown", "x");

    let got = walk_markdown(root.path(), &[PathBuf::from("person")]);
    assert_eq!(got, vec!["person/a.md".to_string()]);
}

#[test]
fn multi_prefix_merge_and_dedup_sorted() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/p.md", "x");
    touch(root.path(), "domains/coding/d.md", "x");
    touch(root.path(), "apps/todo/a.md", "x");
    // 重复前缀 + 交叠前缀（domains 覆盖 domains/coding）→ 去重
    let dirs = vec![
        PathBuf::from("domains"),
        PathBuf::from("person"),
        PathBuf::from("person"),
        PathBuf::from("domains/coding"),
    ];
    let got = walk_markdown(root.path(), &dirs);
    assert_eq!(
        got,
        vec!["domains/coding/d.md".to_string(), "person/p.md".to_string()]
    );
}

#[test]
fn missing_prefix_is_skipped_without_error() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/p.md", "x");

    let got = walk_markdown(
        root.path(),
        &[PathBuf::from("apps/nonexistent"), PathBuf::from("person")],
    );
    assert_eq!(got, vec!["person/p.md".to_string()]);
}

#[test]
fn walk_returns_posix_paths_lexicographic() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/z.md", "x");
    touch(root.path(), "person/sub/nested.md", "x");
    touch(root.path(), "person/a.md", "x");

    let got = walk_markdown(root.path(), &[PathBuf::from("person")]);
    assert_eq!(
        got,
        vec![
            "person/a.md".to_string(),
            "person/sub/nested.md".to_string(),
            "person/z.md".to_string()
        ]
    );
}

#[test]
fn top_level_entries_includes_hidden_and_sorts() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/x.md", "x");
    touch(root.path(), ".dex-ignore", "ignored.md\n");
    fs::create_dir(root.path().join(".git")).unwrap();
    touch(root.path(), "inbox/a.md", "x");

    let got = top_level_entries(root.path());
    // 字典序断言（含隐藏项；'.' < 字母）
    let expected = vec![".dex-ignore", ".git", "inbox", "person"];
    assert_eq!(got, expected);
}

#[test]
fn top_level_entries_missing_root_is_empty() {
    let root = TempDir::new().unwrap();
    assert!(top_level_entries(&root.path().join("nope")).is_empty());
}
