//! search（ripgrep 直扫）集成测试（C3）：多文件命中、limit 截断、scope 标注、
//! 坏正则 → E_BAD_ARGS、大小写敏感缺省。

use dex_core::DexError;
use dex_store::search::{search, Hit};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn touch(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn dirs(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn multi_file_hits_with_scope_and_line_numbers() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/profile.md", "intro\nneedle here\n");
    touch(
        root.path(),
        "domains/coding/rust.md",
        "rust notes\nhas needle too\n",
    );
    touch(root.path(), "domains/coding/noise.md", "nothing relevant\n");

    let hits = search(
        root.path(),
        &dirs(&["person", "domains/coding"]),
        "needle",
        20,
    )
    .unwrap();
    let expect = vec![
        Hit {
            path: "domains/coding/rust.md".into(),
            line: 2,
            scope: "domains/coding".into(),
            content: "has needle too".into(),
        },
        Hit {
            path: "person/profile.md".into(),
            line: 2,
            scope: "person".into(),
            content: "needle here".into(),
        },
    ];
    assert_eq!(hits, expect);
}

#[test]
fn multi_hit_lines_in_one_file_and_trailing_trim() {
    let root = TempDir::new().unwrap();
    touch(
        root.path(),
        "person/notes.md",
        "needle one   \nplain\nneedle two\t\n",
    );

    let hits = search(root.path(), &dirs(&["person"]), "needle", 20).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].content, "needle one");
    assert_eq!(hits[1].line, 3);
    assert_eq!(hits[1].content, "needle two");
}

#[test]
fn limit_truncates_sorted_output() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/a.md", "needle 1\nneedle 2\n");
    touch(root.path(), "person/b.md", "needle 3\n");

    let hits = search(root.path(), &dirs(&["person"]), "needle", 2).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| (h.path.clone(), h.line))
            .collect::<Vec<_>>(),
        vec![
            ("person/a.md".to_string(), 1),
            ("person/a.md".to_string(), 2)
        ]
    );

    // limit 0 → 空
    assert!(search(root.path(), &dirs(&["person"]), "needle", 0)
        .unwrap()
        .is_empty());
}

#[test]
fn only_md_and_ignore_rules_apply() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/in-scope.md", "needle\n");
    touch(root.path(), "person/ignored.md", "needle\n");
    touch(root.path(), "person/notes.txt", "needle\n");
    fs::write(root.path().join(".dex-ignore"), "ignored.md\n").unwrap();

    let hits = search(root.path(), &dirs(&["person"]), "needle", 20).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "person/in-scope.md");
}

#[test]
fn bad_regex_is_bad_args() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/a.md", "x\n");
    let err = search(root.path(), &dirs(&["person"]), "(unclosed", 20).unwrap_err();
    assert_eq!(err.code(), "E_BAD_ARGS");
    assert_eq!(err.exit_code(), 2);
    match &err {
        DexError::BadArgs { message } => assert!(message.contains("正则编译失败"), "{message}"),
        other => panic!("应为 BadArgs：{other:?}"),
    }
}

#[test]
fn default_search_is_case_sensitive() {
    let root = TempDir::new().unwrap();
    touch(
        root.path(),
        "person/case.md",
        "needle lower\nNeedle title\n",
    );

    let hits = search(root.path(), &dirs(&["person"]), "needle", 20).unwrap();
    assert_eq!(hits.len(), 1, "缺省大小写敏感：只命中小写行");
    assert_eq!(hits[0].line, 1);

    // 显式 (?i) 内联标志仍可用（regex 语法面）
    let ci = search(root.path(), &dirs(&["person"]), "(?i)needle", 20).unwrap();
    assert_eq!(ci.len(), 2);
}

#[test]
fn regex_features_supported() {
    let root = TempDir::new().unwrap();
    touch(root.path(), "person/re.md", "go home\ngo house\nstay\n");

    let hits = search(root.path(), &dirs(&["person"]), r"go ho(us|me)", 20).unwrap();
    assert_eq!(hits.len(), 2);
    let anchors = search(root.path(), &dirs(&["person"]), r"^stay$", 20).unwrap();
    assert_eq!(anchors.len(), 1);
}
