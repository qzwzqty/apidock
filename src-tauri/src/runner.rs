use crate::assertions::{self, AssertionResult};
use crate::http::{self, SendOptions};
use crate::storage;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunItem {
    pub group_path: Vec<String>,
    pub key: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub time_ms: Option<u64>,
    pub ok: bool,
    pub error: Option<String>,
    pub assertion_results: Vec<AssertionResult>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub items: Vec<RunItem>,
}

/// 收集接口引用（路径列表）
fn collect(
    nodes: &[storage::TreeNode],
    base: Vec<String>,
    out: &mut Vec<(Vec<String>, String)>,
) {
    for n in nodes {
        match n {
            storage::TreeNode::Group { key, children, .. } => {
                let mut p = base.clone();
                p.push(key.clone());
                collect(children, p, out);
            }
            storage::TreeNode::Interface { key, .. } => {
                out.push((base.clone(), key.clone()));
            }
        }
    }
}

pub async fn run_project(
    root: &Path,
    team_key: &str,
    project_key: &str,
    filter_group: Option<&[String]>,
) -> Result<RunReport, String> {
    let settings = storage::get_project_settings(root, team_key, project_key)?;
    let env_id = settings.active_environment_id.clone().unwrap_or_else(|| "env-prod".into());
    let env = storage::get_environment(root, team_key, project_key, &env_id)?;
    let proxy = storage::read_workspace(root).proxy;

    let tree = storage::list_interface_tree(root, team_key, project_key);
    let mut refs = Vec::new();
    collect(&tree, Vec::new(), &mut refs);

    let mut items = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;

    for (group_path, key) in refs {
        // 分组过滤：空 = 全部；否则匹配指定分组前缀
        if let Some(filter) = filter_group {
            if group_path.len() < filter.len() || group_path[..filter.len()] != filter[..] {
                continue;
            }
        }
        let iface = match storage::get_interface(root, team_key, project_key, &group_path, &key) {
            Ok(i) => i,
            Err(e) => {
                failed += 1;
                items.push(RunItem {
                    group_path,
                    name: key.clone(),
                    key,
                    method: String::new(),
                    url: String::new(),
                    status: None,
                    time_ms: None,
                    ok: false,
                    error: Some(e),
                    assertion_results: Vec::new(),
                });
                continue;
            }
        };
        let opts = SendOptions { proxy: proxy.clone(), ..Default::default() };
        let base = RunItem {
            group_path,
            key,
            name: iface.name.clone(),
            method: iface.method.clone(),
            url: iface.url.clone(),
            status: None,
            time_ms: None,
            ok: false,
            error: None,
            assertion_results: Vec::new(),
        };

        match http::send(&iface, &env, &settings.global_variables, &settings.global_params, &opts).await {
            Ok(resp) => {
                let results = if iface.assertions.is_empty() {
                    Vec::new()
                } else {
                    assertions::check(resp.status, resp.time_ms, &resp.headers, &resp.body, &iface.assertions)
                };
                // 请求成功
                let all_pass = results.iter().all(|r| r.passed);
                let item_ok = all_pass;
                if item_ok {
                    passed += 1;
                } else {
                    failed += 1;
                }
                items.push(RunItem {
                    ok: item_ok,
                    status: Some(resp.status),
                    time_ms: Some(resp.time_ms),
                    assertion_results: results,
                    ..base
                });
            }
            Err(e) => {
                failed += 1;
                items.push(RunItem {
                    ok: false,
                    error: Some(format!("请求失败（{}）：{}", e.kind, e.message)),
                    ..base
                });
            }
        }
    }

    Ok(RunReport {
        total: passed + failed,
        passed,
        failed,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{self, Assertion};

    #[tokio::test]
    async fn run_project_end_to_end() {
        // 本地 HTTP 服务器：/ok -> 200, /bad -> 404，均返回 JSON
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let req = String::from_utf8_lossy(&buf);
                let (code, body) = if req.contains("/bad") {
                    ("404 Not Found", r#"{"code":404}"#)
                } else {
                    ("200 OK", r#"{"code":0}"#)
                };
                let head = format!("HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len());
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(body.as_bytes());
            }
        });

        let root = std::env::temp_dir().join(format!("apidock-runner-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        storage::ensure_root(&root).unwrap();
        storage::create_team(&root, "ops", "运维").unwrap();
        storage::create_project(&root, "ops", "p", "项目").unwrap();

        // 两个接口：一个断言通过，一个断言失败（404 vs 期望 200）
        let mut ok_iface = storage::InterfaceFile::new("ok");
        ok_iface.url = format!("http://{addr}/ok");
        ok_iface.assertions = vec![Assertion::StatusCode { op: "eq".into(), expected: 200 }];
        let mut bad_iface = storage::InterfaceFile::new("bad");
        bad_iface.url = format!("http://{addr}/bad");
        bad_iface.assertions = vec![
            Assertion::StatusCode { op: "eq".into(), expected: 200 },
            Assertion::JsonPath { path: "$.code".into(), op: "eq".into(), expected: "0".into() },
        ];
        storage::create_interface(&root, "ops", "p", &[], "ok", "健康").unwrap();
        storage::create_interface(&root, "ops", "p", &[], "bad", "坏了").unwrap();
        storage::save_interface(&root, "ops", "p", &[], "ok", &ok_iface).unwrap();
        storage::save_interface(&root, "ops", "p", &[], "bad", &bad_iface).unwrap();

        let report = run_project(&root, "ops", "p", None).await.unwrap();
        assert_eq!(report.total, 2);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        let bad = report.items.iter().find(|i| i.key == "bad").unwrap();
        assert!(!bad.ok);
        assert!(!bad.assertion_results.iter().all(|r| r.passed));

        // 分组过滤：空分组路径应只跑该分组
        let report2 = run_project(&root, "ops", "p", Some(&["__none__".to_string()])).await.unwrap();
        assert_eq!(report2.total, 0);

        std::fs::remove_dir_all(&root).unwrap();
    }
}