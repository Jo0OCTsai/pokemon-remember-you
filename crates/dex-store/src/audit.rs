//! 守卫审计日志（C4，FR-10.6 / SECURITY §6）：
//!
//! 冻结契约：
//! - 追加写 `.cache/audit.log`（目录不存在则建；**尽力而为**——任何写失败静默忽略，不阻断主流程）
//! - 行格式（管道分隔）：`UTC ISO-8601｜客户端 id｜命令或工具名｜申请 scope/路径｜结果码`
//! - 字段脱敏：写入前剥离换行与控制字符（防日志行伪造）；**不记录**记忆内容、evidence、token
//! - 单文件软上限 1 MiB：超限保尾轮转（最旧丢弃，从行边界截断）
//!
//! **C 组任务**：实现 + 测试（字段脱敏断言、轮转断言）；公共 API 冻结。

use chrono::{SecondsFormat, Utc};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// 单文件软上限（1 MiB；FR-10.6/SECURITY §6）
pub const ROTATE_MAX_BYTES: usize = 1024 * 1024;

/// 审计日志相对路径（.cache/audit.log）
fn log_path(root: &Path) -> std::path::PathBuf {
    root.join(".cache").join("audit.log")
}

/// 追加一条拒绝事件。client 未注册记 `unknown`；永不返回 Err（尽力而为）。
pub fn append(root: &Path, client: &str, command: &str, requested: &str, code: &str) {
    // 一切 io 错误静默忽略（尽力而为，不阻断主流程）
    let _ = try_append(root, client, command, requested, code);
}

/// 读取现有 audit.log（review/排查辅助；无文件 → 空向量）。
pub fn read_all(root: &Path) -> Vec<String> {
    fs::read_to_string(log_path(root))
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 字段脱敏：\x00-\x1f 与 \x7f → 空格，再 trim（防行伪造与不可见残留）
fn sanitize_field(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if (c as u32) <= 0x1f || c as u32 == 0x7f {
                ' '
            } else {
                c
            }
        })
        .collect();
    cleaned.trim().to_string()
}

fn try_append(
    root: &Path,
    client: &str,
    command: &str,
    requested: &str,
    code: &str,
) -> std::io::Result<()> {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false);
    let line = format!(
        "{ts}|{}|{}|{}|{}\n",
        sanitize_field(client),
        sanitize_field(command),
        sanitize_field(requested),
        sanitize_field(code)
    );
    fs::create_dir_all(root.join(".cache"))?;
    let path = log_path(root);
    {
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        f.write_all(line.as_bytes())?;
    }
    // 超过软上限 → 保尾轮转（最旧丢弃，从行边界截断；新追加行在尾部天然保留）
    let len = fs::metadata(&path)?.len() as usize;
    if len > ROTATE_MAX_BYTES {
        let data = fs::read(&path)?;
        let cut = data.len() - ROTATE_MAX_BYTES;
        let start = data[cut..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(cut, |i| cut + i + 1);
        fs::write(&path, &data[start..])?;
    }
    Ok(())
}
