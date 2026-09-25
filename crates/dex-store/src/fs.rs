//! 文件树遍历（C1）：`ignore` crate；尊重 `.dex-ignore`（语法同 gitignore，FR-1.6）；
//! 顶层白名单口径供 lint 复用。
//!
//! 冻结契约：
//! - [`walk_markdown`]：只收 `.md`，跳过隐藏项（`.` 开头——`.git`/`.cache` 天然排除），
//!   应用仓库根 `.dex-ignore`；返回相对 `root` 的 posix 风格路径，字典序。
//! - [`top_level_entries`]：仓库根顶层条目清单（目录/文件名，含隐藏项）——lint 顶层白名单检查输入。
//!
//! **C 组任务**：实现 + 集成测试（临时目录 fixture）；公共 API 冻结。

use ignore::WalkBuilder;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 遍历 `dirs`（相对 root 的目录前缀，来自 core::scope 展开）下全部 markdown 文件。
pub fn walk_markdown(root: &Path, dirs: &[PathBuf]) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for dir in dirs {
        let base = root.join(dir);
        // 前缀本身可能不存在（scope 展开允许超前声明）——跳过不报错
        if !base.is_dir() {
            continue;
        }
        // parents(true)（缺省）使 walk root 的祖先目录中的 .dex-ignore 同样生效——
        // 仓库根 .dex-ignore 因此覆盖所有前缀遍历
        let walker = WalkBuilder::new(&base)
            .add_custom_ignore_filename(".dex-ignore")
            .git_ignore(false)
            .git_exclude(false)
            .git_global(false)
            .require_git(false)
            .hidden(true)
            .build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(root) {
                set.insert(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    set.into_iter().collect()
}

/// 仓库根顶层条目（文件与目录名，含隐藏项如 .git/.dex-ignore）。
pub fn top_level_entries(root: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}
