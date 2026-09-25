//! git 集成测试（C2）：tempfile 临时仓库 fixture——init/commit/snapshot/dirty/
//! index.lock 退避（退避间隔缩短为 1ms）/ git_missing 判定。

use dex_core::DexError;
use dex_store::git::{
    available, commit_all, git_missing, init_and_first_commit, is_repo, last_touch_snapshot,
    status_dirty, CommitOutcome,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tempfile::TempDir;

/// index.lock 用例互斥：两例都改写全局退避钩子 LOCK_BACKOFF_MS，须串行防互相干扰
static LOCK_TESTS: Mutex<()> = Mutex::new(());

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// 历史提交 fixture：固定 2026-01-01 作者/提交者日期
fn commit_at(root: &Path, date_env: &str, msg: &str) {
    let status = Command::new("git")
        .current_dir(root)
        .env("GIT_AUTHOR_DATE", date_env)
        .env("GIT_COMMITTER_DATE", date_env)
        .args([
            "-c",
            "user.name=dex",
            "-c",
            "user.email=dex@local",
            "commit",
            "-m",
            msg,
        ])
        .status()
        .unwrap();
    assert!(status.success(), "fixture commit 失败：{msg}");
}

#[test]
fn available_detects_git_and_version() {
    // 测试环境须有 git（缺 git 的环境该用例退化为断言 false——本仓 CI 有 git）
    assert!(available());
}

#[test]
fn init_and_first_commit_creates_repo() {
    let root = TempDir::new().unwrap();
    assert!(!is_repo(root.path()));
    let created = init_and_first_commit(root.path(), "dex init 首次提交").unwrap();
    assert!(created, "首次应新建仓库");
    assert!(is_repo(root.path()));
    // 幂等：已存在 → Ok(false)，不再额外建仓
    let again = init_and_first_commit(root.path(), "再次 init").unwrap();
    assert!(!again);
}

#[test]
fn commit_all_and_nothing_to_commit_and_dirty() {
    let root = TempDir::new().unwrap();
    init_and_first_commit(root.path(), "init").unwrap();

    // 初始（仅空骨架 commit）→ 干净
    assert!(!status_dirty(root.path()));
    assert_eq!(
        commit_all(root.path(), "无变更").unwrap(),
        CommitOutcome::NothingToCommit
    );

    // 写文件 → dirty → commit → Committed → 干净
    write(root.path(), "person/a.md", "hello\n");
    assert!(status_dirty(root.path()));
    assert_eq!(
        commit_all(root.path(), "add a").unwrap(),
        CommitOutcome::Committed
    );
    assert!(!status_dirty(root.path()));

    // 修改 + 删除混合变更
    write(root.path(), "person/a.md", "hello v2\n");
    write(root.path(), "person/b.md", "b\n");
    assert_eq!(
        commit_all(root.path(), "modify").unwrap(),
        CommitOutcome::Committed
    );
    assert!(!status_dirty(root.path()));
}

#[test]
fn snapshot_single_pass_dates_and_untracked_excluded() {
    let root = TempDir::new().unwrap();
    init_and_first_commit(root.path(), "init").unwrap();

    // 旧提交（2026-01-01）：a.md、b.md
    write(root.path(), "person/a.md", "v1\n");
    write(root.path(), "person/b.md", "v1\n");
    Command::new("git")
        .current_dir(root.path())
        .args(["add", "-A"])
        .status()
        .unwrap();
    commit_at(root.path(), "2026-01-01T00:00:00", "old");

    // 新提交（今天）：修改 a.md（b.md 不再触及）+ 新增 c.md
    write(root.path(), "person/a.md", "v2\n");
    write(root.path(), "person/c.md", "new\n");
    commit_all(root.path(), "new").unwrap();

    // 未跟踪文件：不在 snapshot
    write(root.path(), "person/untracked.md", "no commit\n");

    let snap = last_touch_snapshot(root.path()).unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let expect: HashMap<String, String> = [
        ("person/a.md".to_string(), today.clone()),
        ("person/b.md".to_string(), "2026-01-01".to_string()),
        ("person/c.md".to_string(), today),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        snap, expect,
        "a.md 应定格于最新触及（today），b.md 保持旧日期，未跟踪文件排除"
    );
}

#[test]
fn snapshot_empty_repo_and_non_repo() {
    // 空仓库（init 无提交）→ 空表
    let root = TempDir::new().unwrap();
    Command::new("git")
        .current_dir(root.path())
        .args(["init", "-q"])
        .status()
        .unwrap();
    assert!(last_touch_snapshot(root.path()).unwrap().is_empty());

    // 非仓库 → E_REPO_STATE
    let plain = TempDir::new().unwrap();
    let err = last_touch_snapshot(plain.path()).unwrap_err();
    assert_eq!(err.code(), "E_REPO_STATE");
    assert_eq!(err.exit_code(), 8);
}

#[test]
fn index_lock_backoff_exceeds_to_repo_state() {
    let _guard = LOCK_TESTS.lock().unwrap();
    let root = TempDir::new().unwrap();
    init_and_first_commit(root.path(), "init").unwrap();
    write(root.path(), "person/x.md", "staged under lock\n");

    // 造持久 index.lock（重试期间无人释放）→ 退避重试后仍锁 → E_REPO_STATE
    let lock = root.path().join(".git").join("index.lock");
    fs::write(&lock, b"locked").unwrap();
    // 缩短退避（50ms → 1ms；10 次重试 ≈ 10ms 而非 500ms）
    dex_store::git::LOCK_BACKOFF_MS.store(1, Ordering::SeqCst);

    let err = commit_all(root.path(), "should fail").unwrap_err();
    assert!(
        matches!(&err, DexError::RepoState { message } if message.contains("index.lock 冲突重试超限")),
        "应为 index.lock 重试超限错误，实际：{err:?}"
    );
    assert_eq!(err.code(), "E_REPO_STATE");
}

#[test]
fn index_lock_released_during_backoff_recovers() {
    let _guard = LOCK_TESTS.lock().unwrap();
    let root = TempDir::new().unwrap();
    init_and_first_commit(root.path(), "init").unwrap();
    write(
        root.path(),
        "person/y.md",
        "will commit after lock released\n",
    );

    let lock = root.path().join(".git").join("index.lock");
    fs::write(&lock, b"locked").unwrap();
    // 确定退避 20ms（防上一用例遗留的 1ms）
    dex_store::git::LOCK_BACKOFF_MS.store(20, Ordering::SeqCst);

    // 后台在若干毫秒后释放锁 → 退避重试应恢复成功
    let lock_path = lock.clone();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(60));
        fs::remove_file(&lock_path).unwrap();
    });
    let outcome = commit_all(root.path(), "recovered").unwrap();
    release.join().unwrap();
    assert_eq!(outcome, CommitOutcome::Committed);
    assert!(!status_dirty(root.path()));
}

#[test]
fn git_missing_predicate() {
    let missing = DexError::RepoState {
        message: "git 不可用（PATH 未找到）".into(),
    };
    assert!(git_missing(&missing));
    let other = DexError::RepoState {
        message: "index.lock 冲突重试超限".into(),
    };
    assert!(!git_missing(&other));
    assert!(!git_missing(&DexError::BadArgs {
        message: "x".into()
    }));
}

#[test]
fn commit_all_message_and_identity_recorded() {
    let root = TempDir::new().unwrap();
    init_and_first_commit(root.path(), "init").unwrap();
    write(root.path(), "person/id.md", "identity check\n");
    assert_eq!(
        commit_all(root.path(), "propose: claude").unwrap(),
        CommitOutcome::Committed
    );

    // 提交身份统一为 dex（实现决策）；提交信息保留原文
    let out = Command::new("git")
        .current_dir(root.path())
        .args(["log", "-1", "--pretty=format:%an|%ae|%s"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "dex|dex@local|propose: claude"
    );
}
