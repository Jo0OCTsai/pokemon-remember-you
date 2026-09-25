//! `dex read <relpath> [--section]`（FR-6.2 / §8.1）：读单文件/小节，头部附 scope 标注。
//!
//! 冻结语义：
//! - 路径安全（§10）：拒绝绝对路径、`..` 段、`\`、symlink 逃逸出 DEX_ROOT、
//!   `.git`/`.cache`/`.obsidian`/`.dex` 禁区 → E_BAD_PATH · 6（写 audit）
//! - 读授权：路径解析出的 scope 须 ⊆ 客户端白名单（journal/inbox 属显式授权面——
//!   白名单含该 scope 或 human 全量才放行，FR-3.1）；越权整单拒绝 E_SCOPE_DENIED · 3
//! - `--section H2标题`：输出该 `## ` 小节（至下一 `## ` 或 EOF）
//! - 退出码 0 / 1 文件不存在（E_NOT_FOUND）/ 3 越权 / 6 路径非法

use crate::cmd::CommonOpts;
use crate::guard_runtime::Ctx;
use clap::Args;
use dex_core::scope::scope_of_path;
use dex_core::DexError;
use serde_json::json;
use std::path::{Component, Path};

/// 首段禁区（§10）
const FORBIDDEN_FIRST: [&str; 4] = [".git", ".cache", ".obsidian", ".dex"];

#[derive(Args, Debug)]
pub struct ReadArgs {
    #[command(flatten)]
    pub common: CommonOpts,
    /// 仓库内相对路径
    pub path: String,
    /// H2 小节标题（不含 `## ` 前缀）
    #[arg(long)]
    pub section: Option<String>,
}

pub fn run(args: &ReadArgs, ctx: &Ctx) -> Result<i32, DexError> {
    let rel = args.path.as_str();
    let rel_path = Path::new(rel);

    // 1. 静态路径安全（§10）：空 / 绝对路径 / `..` 段 / `\` / 首段禁区 → E_BAD_PATH · 6（audit）
    let first = rel_path
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str());
    let bad = rel.is_empty()
        || rel_path.is_absolute()
        || rel.contains('\\')
        || rel_path
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        || first.is_some_and(|f| FORBIDDEN_FIRST.contains(&f));
    if bad {
        ctx.audit(rel, "E_BAD_PATH");
        return Err(DexError::BadPath {
            path: rel.to_string(),
        });
    }

    // 2. 存在性分支（先于 canonicalize——目标不存在时 canonicalize 失败）→ E_NOT_FOUND · 1
    let target = ctx.root.join(rel_path);
    if !target.exists() {
        return Err(DexError::NotFound {
            what: format!("文件不存在：{rel}"),
        });
    }

    // 3. symlink 逃逸：canonicalize（根取真实路径，防 /var ↔ /private/var 误判）后必须仍在根内
    //    → E_BAD_PATH · 6（audit）
    let root_canon = ctx.root.canonicalize().unwrap_or_else(|_| ctx.root.clone());
    let target_canon = target.canonicalize().map_err(|_| DexError::NotFound {
        what: format!("文件不存在：{rel}"),
    })?;
    if !target_canon.starts_with(&root_canon) {
        ctx.audit(rel, "E_BAD_PATH");
        return Err(DexError::BadPath {
            path: rel.to_string(),
        });
    }
    if !target_canon.is_file() {
        return Err(DexError::NotFound {
            what: format!("不是常规文件：{rel}"),
        });
    }

    // 4. 读授权（FR-3.1 显式授权面）：scope_of_path → None（白名单外顶层/`.` 开头）→ 拒绝；
    //    Some(scope) 须 ⊆ 白名单（journal/inbox 同规则——白名单含该 scope 或 human 全量）
    let Some(scope) = scope_of_path(rel_path) else {
        ctx.audit(rel, "E_SCOPE_DENIED");
        return Err(DexError::ScopeDenied {
            detail: format!("路径 {rel:?} 不在任何 scope 内"),
        });
    };
    if !ctx.grant().allows(&scope) {
        ctx.audit(rel, "E_SCOPE_DENIED");
        return Err(DexError::ScopeDenied {
            detail: format!("路径 {rel} 的 scope {scope} 超出客户端白名单（整单拒绝）"),
        });
    }

    // 5. 读原始内容（不做净化——净化仅 render 注入产物，FR-6.16）
    let content = std::fs::read_to_string(&target).map_err(|_| DexError::NotFound {
        what: format!("文件不可读或非 UTF-8：{rel}"),
    })?;

    // 6. --section：`## <title>` 起（含标题行）至下一 `## ` 行或 EOF；标题找不到 → E_NOT_FOUND · 1
    let section_text: Option<String> = match &args.section {
        Some(title) => {
            Some(
                extract_section(&content, title).ok_or_else(|| DexError::NotFound {
                    what: format!("小节不存在：{title}"),
                })?,
            )
        }
        None => None,
    };
    let body = section_text.as_deref().unwrap_or(&content);

    // 7. 输出：text 首行 `# scope: <scope>` 后接内容（或小节内容）；
    //    json data = {path, scope, section: Option, content}
    let data = json!({
        "path": rel,
        "scope": scope.as_str(),
        "section": args.section,
        "content": body,
    });
    let mut text = format!("# scope: {}\n", scope);
    text.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        text.push('\n');
    }
    Ok(ctx.emit(0, Vec::new(), data, text))
}

/// 抽取 `## <title>` 小节：含标题行，至下一 `## ` 行或 EOF（`###` 子标题不切断）
fn extract_section(content: &str, title: &str) -> Option<String> {
    let heading = format!("## {title}");
    let mut out = String::new();
    let mut in_section = false;
    for line in content.lines() {
        let line = line.trim_end();
        if in_section && line.starts_with("## ") {
            break;
        }
        if !in_section && line == heading {
            in_section = true;
        }
        if in_section {
            out.push_str(line);
            out.push('\n');
        }
    }
    if in_section {
        Some(out)
    } else {
        None
    }
}
