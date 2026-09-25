//! `--json` 统一信封（§8.4）：`{ok, warnings[], data}` / `{ok:false, error{code,message}}`。
//! 键序经 serde_json 默认（字典序）确定——机器可依赖、跨运行字节一致。

use dex_core::{DexError, Warning};
use serde_json::{json, Value};

pub fn warnings_value(ws: &[Warning]) -> Value {
    Value::Array(
        ws.iter()
            .map(|w| json!({ "code": w.code(), "message": w.message() }))
            .collect(),
    )
}

pub fn print_ok(warnings: &[Warning], data: Value) {
    let obj = json!({ "ok": true, "warnings": warnings_value(warnings), "data": data });
    println!("{obj}");
}

pub fn print_err(err: &DexError) {
    let obj = json!({ "ok": false, "error": { "code": err.code(), "message": err.to_string() } });
    println!("{obj}");
}
