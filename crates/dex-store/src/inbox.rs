//! inbox 视图与收割暂存（C4）：实现 [`dex_core::guard::InboxView`] 端口 + staging 管理。
//!
//! 冻结契约：
//! - inbox 顶层 = 提案区（`YYYY-MM-DD-<source>-<shortid>.md`）；`inbox/staging/<source>/`
//!   为收割暂存（git 跟踪、豁免滞留 lint，转正时全量走守卫，FR-6.14）；
//!   `inbox/bootstrap/` 为 v0 遗留（lint 提示迁移）
//! - [`Inbox`] 只读视图：count_today / today_filenames / pending（frontmatter 解析失败的
//!   文件跳过并在 pending 中忽略——坏文件由 lint 报告）
//! - staging 操作：list_staging / stage_write / promote_move（移动 + 由调用方触发守卫与提交）
//!
//! 错误口径（C 组实现决策）：staging/promote 的目录创建与移动失败属仓库/文件系统状态错误
//! → E_REPO_STATE；promote 源文件不存在 → E_NOT_FOUND（语义性缺失）。
//!
//! **C 组任务**：实现 + 测试；公共 API 冻结。

use dex_core::guard::{InboxView, PendingProposal};
use dex_core::proposal::{parse_filename_date_source, parse_proposal_file};
use dex_core::DexError;
use std::path::{Path, PathBuf};

/// inbox 只读视图（root = 仓库根）
pub struct Inbox {
    pub root: PathBuf,
}

impl Inbox {
    pub fn new(root: &Path) -> Self {
        Inbox {
            root: root.to_path_buf(),
        }
    }

    fn inbox_dir(&self) -> PathBuf {
        self.root.join("inbox")
    }

    /// inbox 顶层全部提案文件名（排序；不含 staging/bootstrap 子目录）。
    pub fn list_top_level(&self) -> Vec<String> {
        read_dir_names(&self.inbox_dir(), |name| name.ends_with(".md"))
    }

    /// staging 目录：`inbox/staging/<source>/`
    pub fn staging_dir(&self, source: &str) -> PathBuf {
        self.inbox_dir().join("staging").join(source)
    }

    /// staging 内候选文件清单（排序；文件名自由，内容为提案 frontmatter 格式）。
    pub fn list_staging(&self, source: &str) -> Vec<PathBuf> {
        let dir = self.staging_dir(source);
        if !dir.is_dir() {
            return Vec::new();
        }
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !name.starts_with('.')
            })
            .map(|e| e.path())
            .collect();
        paths.sort();
        paths
    }

    /// 写入 staging 候选文件（`inbox/staging/<source>/<name>.md`；目录不存在则建）。
    pub fn stage_write(
        &self,
        source: &str,
        name: &str,
        content: &str,
    ) -> Result<PathBuf, DexError> {
        let dir = self.staging_dir(source);
        std::fs::create_dir_all(&dir).map_err(|e| DexError::RepoState {
            message: format!("staging 目录创建失败：{e}"),
        })?;
        let path = dir.join(format!("{name}.md"));
        std::fs::write(&path, content).map_err(|e| DexError::RepoState {
            message: format!("staging 写入失败：{e}"),
        })?;
        Ok(path)
    }

    /// 把 staging 文件移动为 inbox 顶层提案（重命名为标准命名式；文件系统 move）。
    /// `staged_name` = staging 内实际文件名；`new_filename` = inbox 顶层目标文件名。
    pub fn promote_move(
        &self,
        source: &str,
        staged_name: &str,
        new_filename: &str,
    ) -> Result<PathBuf, DexError> {
        let from = self.staging_dir(source).join(staged_name);
        let to = self.inbox_dir().join(new_filename);
        if !from.exists() {
            return Err(DexError::NotFound {
                what: format!("staging 候选不存在：{staged_name}"),
            });
        }
        std::fs::rename(&from, &to).map_err(|e| DexError::RepoState {
            message: format!("promote 移动失败：{e}"),
        })?;
        Ok(to)
    }

    /// v0 遗留 `inbox/bootstrap/` 是否存在及其文件数（lint 迁移提示用）。
    pub fn legacy_bootstrap(&self) -> Option<usize> {
        let dir = self.inbox_dir().join("bootstrap");
        if !dir.is_dir() {
            return None;
        }
        Some(read_dir_names(&dir, |name| name.ends_with(".md")).len())
    }

    /// 今日该 source 的顶层文件名列表（命名式反解析过滤）
    fn matching_top_level(&self, source: &str, date: &str) -> Vec<String> {
        self.list_top_level()
            .into_iter()
            .filter(|name| {
                parse_filename_date_source(name).is_some_and(|(d, s)| d == date && s == source)
            })
            .collect()
    }
}

impl InboxView for Inbox {
    fn count_today(&self, source: &str, date: &str) -> Result<usize, DexError> {
        Ok(self.matching_top_level(source, date).len())
    }

    fn today_filenames(&self, source: &str, date: &str) -> Result<Vec<String>, DexError> {
        Ok(self.matching_top_level(source, date))
    }

    fn pending(&self) -> Result<Vec<PendingProposal>, DexError> {
        let mut out = Vec::new();
        for name in self.list_top_level() {
            let path = self.inbox_dir().join(&name);
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            // 解析失败文件跳过（坏文件由 lint 报告，冻结契约）
            if let Ok((meta, content)) = parse_proposal_file(&raw) {
                out.push(PendingProposal {
                    path,
                    source: meta.source,
                    kind: meta.kind,
                    content,
                });
            }
        }
        Ok(out)
    }
}

/// 读目录一层：收集（过滤后的）文件名并排序。目录不存在/读失败 → 空表。
fn read_dir_names(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        // 只要普通文件（staging/bootstrap 子目录天然排除）
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| keep(n))
        .collect();
    names.sort();
    names
}
