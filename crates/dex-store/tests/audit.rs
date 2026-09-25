//! audit 集成测试（C4）：字段脱敏（控制字符/换行）、行格式字段数、
//! >1 MiB 保尾轮转（真实阈值，写 1.2 MiB 数据）。

use dex_store::audit::{append, read_all, ROTATE_MAX_BYTES};
use std::fs;
use tempfile::TempDir;

#[test]
fn append_writes_five_pipe_separated_fields() {
    let root = TempDir::new().unwrap();
    append(
        root.path(),
        "claude-code",
        "search",
        "domains/coding",
        "E_SCOPE_DENIED",
    );
    let lines = read_all(root.path());
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    // 字段数 = 5（UTC｜client｜command｜requested｜code），字段本体不含 '|'
    assert_eq!(line.split('|').count(), 5);
    let fields: Vec<&str> = line.split('|').collect();
    assert_eq!(fields[1], "claude-code");
    assert_eq!(fields[2], "search");
    assert_eq!(fields[3], "domains/coding");
    assert_eq!(fields[4], "E_SCOPE_DENIED");
    // UTC 时间戳形如 2026-09-25T12:34:56.123456+00:00（RFC3339 微秒）
    assert!(fields[0].starts_with("20"), "{}", fields[0]);
    assert!(
        fields[0].contains('T') && fields[0].contains('+'),
        "{}",
        fields[0]
    );
}

#[test]
fn control_chars_and_newlines_are_sanitized() {
    let root = TempDir::new().unwrap();
    append(
        root.path(),
        "client\nINJECTED",
        "cmd\x00x",
        "path\r\nfake-line|E_OK",
        "E_SCOPE_DENIED",
    );
    let lines = read_all(root.path());
    // 换行被剥离 ⇒ 单行；不产生伪造行
    assert_eq!(lines.len(), 1, "换行不得进日志行：{lines:?}");
    assert!(!lines[0].contains('\n'));
    assert!(!lines[0].contains('\r'));
    assert!(!lines[0].contains('\x00'));
    // '|' 属普通字符不脱敏（防行伪造只针对控制字符）⇒ requested 内的管道保留为第 6 段
    assert_eq!(lines[0].split('|').count(), 6, "行：{:?}", lines[0]);
}

#[test]
fn append_creates_cache_dir_and_accumulates() {
    let root = TempDir::new().unwrap();
    append(root.path(), "a", "read", "person", "E_SCOPE_DENIED");
    append(root.path(), "b", "write", "inbox", "E_RATE_LIMIT");
    assert!(root.path().join(".cache/audit.log").is_file());
    let lines = read_all(root.path());
    assert_eq!(lines.len(), 2);
    assert!(lines[1].ends_with("|E_RATE_LIMIT"));
}

#[test]
fn read_all_missing_file_is_empty() {
    let root = TempDir::new().unwrap();
    assert!(read_all(root.path()).is_empty());
}

#[test]
fn rotation_keeps_tail_over_1mib_from_line_boundary() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join(".cache");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("audit.log");

    // 预写 ~1.2 MiB 合法审计行（每行 ~120B × 10_000 ≈ 1.2 MiB）
    let filler = "x".repeat(96);
    let mut bulk = String::new();
    for i in 0..10_000 {
        bulk.push_str(&format!(
            "2026-01-01T00:00:00.000000+00:00|bulk|cmd|{filler}|{i:06}\n"
        ));
    }
    fs::write(&path, &bulk).unwrap();
    assert!(fs::metadata(&path).unwrap().len() as usize > ROTATE_MAX_BYTES);

    // 触发 append → 超限保尾轮转
    append(
        root.path(),
        "claude",
        "search",
        "domains/coding",
        "E_SCOPE_DENIED",
    );

    let lines = read_all(root.path());
    let size = fs::metadata(&path).unwrap().len() as usize;
    // 尾部保留：新行在最后一行；体积回落到 ≈1 MiB（≤ 阈值 + 最后一行余量）
    assert!(size <= ROTATE_MAX_BYTES, "轮转后应 ≤ 1 MiB，实际 {size}");
    assert!(!lines.is_empty());
    let last = lines.last().unwrap();
    assert!(last.ends_with("|E_SCOPE_DENIED"), "追加行须保留：{last}");

    // 最旧内容被丢弃（bulk 开头序号不再出现）
    assert!(
        !lines.first().unwrap().ends_with("|000000"),
        "最旧行应已丢弃"
    );
    // 首行是完整记录（行边界截断：5 字段 + 以合法年份开头）
    let first = lines.first().unwrap();
    assert_eq!(
        first.split('|').count(),
        5,
        "轮转后首行应为完整记录：{first}"
    );
    assert!(first.starts_with("20"), "首行应以时间戳开头：{first}");

    // 保留的正是尾部连续段：首行序号 > 丢弃数量级
    let first_idx: usize = first.rsplit('|').next().unwrap().parse().unwrap();
    assert!(
        first_idx > 1_000,
        "应保留尾部（首行序号 {first_idx} 应明显靠后）"
    );
}

#[test]
fn no_rotation_below_threshold() {
    let root = TempDir::new().unwrap();
    append(root.path(), "claude", "read", "person", "E_SCOPE_DENIED");
    // 小文件不轮转：内容原样保留
    let lines = read_all(root.path());
    assert_eq!(lines.len(), 1);
}
