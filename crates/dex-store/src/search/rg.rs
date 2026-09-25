//! ripgrep 直扫后端（C3）：`grep-regex` + `grep-searcher`（静态链接进二进制，NFR-5 口径）。
//!
//! 冻结契约：
//! - 输出 `path:line:scope:content`（text 模式行格式由 cli 拼；此处结构化返回）
//! - `--limit` 封顶 20（§8.1；cli 层 clamp，本层尊重传入值）
//! - 正则编译失败 → E_BAD_ARGS（未知 pattern 语法）
//! - 只扫 `.md`；`.dex-ignore` 生效（复用 fs::walk_markdown 的遍历口径）
//!
//! **C 组任务**：实现 + 测试；公共 API 冻结。

use crate::fs;
use dex_core::scope::scope_of_path;
use dex_core::DexError;
use grep_regex::RegexMatcher;
use grep_searcher::sinks::Lossy;
use grep_searcher::Searcher;
use std::path::{Path, PathBuf};

/// 检索命中
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 仓库相对路径（posix 风格）
    pub path: String,
    /// 1-based 行号
    pub line: usize,
    /// 命中行所在 scope（`person` / `domains/coding` …；scope_of_path 派生）
    pub scope: String,
    /// 命中行原文（去行尾换行；不截断）
    pub content: String,
}

/// 在 `dirs`（相对 root 的目录前缀）下正则检索，最多 `limit` 条。
pub fn search(
    root: &Path,
    dirs: &[PathBuf],
    pattern: &str,
    limit: usize,
) -> Result<Vec<Hit>, DexError> {
    let matcher = RegexMatcher::new(pattern).map_err(|e| DexError::BadArgs {
        message: format!("正则编译失败：{e}"),
    })?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    // walk_markdown 已字典序 ⇒ 逐文件追加天然按 (path, line) 有序；末尾再排序兜底
    let mut hits: Vec<Hit> = Vec::new();
    'outer: for rel in fs::walk_markdown(root, dirs) {
        let scope = scope_of_path(Path::new(&rel))
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();
        let mut matched: Vec<(usize, String)> = Vec::new();
        let sink = Lossy(|line_num: u64, line: &str| {
            matched.push((line_num as usize, line.trim_end().to_string()));
            Ok(true)
        });
        // 读失败/损坏文件静默跳过（检索尽力而为）
        if Searcher::new()
            .search_path(&matcher, root.join(&rel), sink)
            .is_err()
        {
            continue;
        }
        for (line, content) in matched {
            hits.push(Hit {
                path: rel.clone(),
                line,
                scope: scope.clone(),
                content,
            });
            if hits.len() >= limit {
                break 'outer;
            }
        }
    }
    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    hits.truncate(limit);
    Ok(hits)
}
