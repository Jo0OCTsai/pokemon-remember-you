//! E3 输出一致性（PLAN E3）：同一操作 `--json` 与 text 输出断言等价
//! （v2 才有 MCP 通道一致性；本期锁 CLI 双模式一致性——换模式不换语义）。

mod common;

use common::*;
use std::fs;

fn parse_text_hits(text: &str) -> Vec<(String, u64, String, String)> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            // path:line:scope:content —— path 与 content 均不含冒号分隔歧义时按前三个 `:` 切
            let parts: Vec<&str> = l.splitn(4, ':').collect();
            assert_eq!(parts.len(), 4, "text 行格式 path:line:scope:content：{l}");
            (
                parts[0].to_string(),
                parts[1].parse().unwrap(),
                parts[2].to_string(),
                parts[3].to_string(),
            )
        })
        .collect()
}

#[test]
fn search_text_json_equivalent() {
    let env = Env::new();
    env.standard_config();
    let json_out = env
        .dex_with_token(
            &["search", "部署|偏好", "--json", "--client", "choose-you"],
            Some(SPOKE_TOKEN),
        )
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text_out = env
        .dex_with_token(
            &["search", "部署|偏好", "--client", "choose-you"],
            Some(SPOKE_TOKEN),
        )
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();

    let v: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
    let results = v["data"]["results"].as_array().unwrap();
    let text_hits = parse_text_hits(&String::from_utf8_lossy(&text_out));
    assert_eq!(results.len(), text_hits.len(), "两模式命中数一致");
    for (r, t) in results.iter().zip(&text_hits) {
        assert_eq!(r["path"].as_str().unwrap(), t.0);
        assert_eq!(r["line"].as_u64().unwrap(), t.1);
        assert_eq!(r["scope"].as_str().unwrap(), t.2);
        assert_eq!(r["content"].as_str().unwrap(), t.3);
    }
    assert_eq!(v["data"]["count"].as_u64().unwrap(), text_hits.len() as u64);
}

#[test]
fn no_hit_error_envelope_equivalent() {
    let env = Env::new();
    env.standard_config();
    let json_out = env
        .dex_with_token(
            &[
                "search",
                "绝不存在的词xyzq",
                "--json",
                "--client",
                "choose-you",
            ],
            Some(SPOKE_TOKEN),
        )
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
    assert_eq!(v["ok"], serde_json::json!(false));
    assert_eq!(v["error"]["code"], "E_NOT_FOUND");
    // text 模式同样退出码 1
    env.dex_with_token(
        &["search", "绝不存在的词xyzq", "--client", "choose-you"],
        Some(SPOKE_TOKEN),
    )
    .assert()
    .code(1);
}

#[test]
fn propose_text_json_equivalent() {
    let env = Env::new();
    env.standard_config();
    let args = [
        "propose",
        "--source",
        "choose-you",
        "--kind",
        "pattern",
        "--evidence",
        "consistency #1",
        "一致性正文",
        "--client",
        "choose-you",
    ];
    let json_out = env
        .dex_with_token(
            &{
                let mut a = args.to_vec();
                a.push("--json");
                a
            },
            Some(SPOKE_TOKEN),
        )
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
    let file = v["data"]["file"].as_str().unwrap().to_string();

    // 第二次（stdin `-` 形态、同内容）为幂等重放：text 输出既有文件路径，与 json 首次 data.file 一致
    let stdin_args: Vec<&str> = vec![
        "propose",
        "--source",
        "choose-you",
        "--kind",
        "pattern",
        "--evidence",
        "consistency #2",
        "-",
        "--client",
        "choose-you",
    ];
    let text_out = env
        .dex_with_token(&stdin_args, Some(SPOKE_TOKEN))
        .write_stdin("一致性正文")
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    // 第二次为幂等重放：text 输出既有文件路径（与 json 首次的 data.file 一致）
    assert!(
        String::from_utf8_lossy(&text_out).contains(&file),
        "text 模式幂等路径与 json 模式一致：{file}"
    );
    assert!(env.path(&file).exists());
    assert_eq!(fs::read_dir(env.path("inbox")).unwrap().count(), 1);
}

#[test]
fn lint_text_json_equivalent() {
    let env = Env::new();
    env.standard_config();
    // 构造 error 级问题：顶层白名单外目录
    fs::create_dir_all(env.path("evil-dir")).unwrap();

    let json_out = env
        .admin(&["lint", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
    // 信封特例：发现问题 ⇒ ok=true + data.findings + 退出码 1（§8.4）
    assert_eq!(v["ok"], serde_json::json!(true), "lint 信封特例 ok=true");
    let findings = v["data"]["findings"].as_array().unwrap();
    assert!(findings
        .iter()
        .any(|f| f["check"] == "top-level-whitelist" && f["level"] == "error"));

    let text_out = env
        .admin(&["lint"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&text_out);
    assert!(
        text.contains("top-level-whitelist"),
        "text 模式含同一 check 名"
    );
    let text_err_count = text.lines().filter(|l| l.starts_with("[error]")).count();
    assert_eq!(
        text_err_count,
        findings.iter().filter(|f| f["level"] == "error").count(),
        "两模式 error 级计数一致"
    );
}

#[test]
fn stale_text_json_equivalent() {
    let env = Env::new();
    env.standard_config();
    let json_out = env
        .admin(&["stale", "--json"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text_out = env
        .admin(&["stale"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
    let count = v["data"]["count"].as_u64().unwrap();
    let text_lines = String::from_utf8_lossy(&text_out)
        .lines()
        .filter(|l| l.contains('|') && !l.starts_with("⚠"))
        .count() as u64;
    assert_eq!(
        count, text_lines,
        "stale 两模式候选数一致（新鲜仓库应为 0）"
    );
}
