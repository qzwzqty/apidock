use crate::storage;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

/// 从一个外部文档解析出的“待导入接口”
#[derive(Debug, Clone)]
pub struct ImportedIface {
    pub group_path: Vec<String>,
    pub key: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<storage::KeyValue>,
    pub query: Vec<storage::KeyValue>,
    pub body: storage::Body,
    pub auth: storage::Auth,
    pub description: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub total: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

impl ImportedIface {
    pub fn to_file(&self) -> storage::InterfaceFile {
        storage::InterfaceFile::new(&self.key)
    }
}

fn sanitize(value: &str) -> String {
    crate::storage::sanitize_key(value)
}

/// 解析 OpenAPI 3.x（JSON 或 YAML）→ 待导入接口列表
pub fn parse_openapi(content: &str, is_yaml: bool) -> Result<(String, Vec<ImportedIface>), String> {
    let doc: Value = if is_yaml {
        serde_yaml_ng::from_str(content).map_err(|e| format!("YAML 解析失败：{e}"))
    } else {
        serde_json::from_str(content).map_err(|e| format!("JSON 解析失败：{e}"))
    }?;

    let info_name = doc
        .get("info")
        .and_then(|i| i.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("导入的项目")
        .to_string();

    let mut schemes: Vec<(String, &str)> = Vec::new();
    let mut security_required: Option<String> = None;
    if let Some(ss) = doc.pointer("/components/securitySchemes") {
        if let Some(obj) = ss.as_object() {
            for (name, scheme) in obj {
                let kind = match scheme.get("type").and_then(|t| t.as_str()) {
                    Some("http") => match scheme.get("scheme").and_then(|s| s.as_str()) {
                        Some("basic") => "basic",
                        Some("bearer") => "bearer",
                        _ => "",
                    },
                    Some("apiKey") => "api-key",
                    _ => "",
                };
                if !kind.is_empty() {
                    schemes.push((name.clone(), kind));
                }
            }
        }
    }
    if let Some(sec) = doc.get("security").and_then(|s| s.as_array()) {
        for entry in sec {
            if let Some(obj) = entry.as_object() {
                if let Some((name, _)) = obj.iter().next() {
                    if let Some((_, kind)) = schemes.iter().find(|(n, _)| n == name) {
                        security_required = Some(kind.to_string());
                    }
                }
            }
        }
    }

    let server_base = doc
        .get("servers")
        .and_then(|s| s.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .trim()
        .trim_end_matches('/')
        .to_string();

    let mut list: Vec<ImportedIface> = Vec::new();
    if let Some(paths) = doc.get("paths").and_then(|p| p.as_object()) {
        for (path, path_item) in paths {
            if path == "components" || path.starts_with("x-") {
                continue;
            }
            let Some(pt) = path_item.as_object() else { continue };
            for (method, op) in pt {
                let method = method.to_lowercase();
                if !["get", "post", "put", "patch", "delete", "head", "options", "trace"].contains(&method.as_str()) {
                    continue;
                }
                let Some(op) = op.as_object() else { continue };

                let name = op
                    .get("summary")
                    .or_else(|| op.get("operationId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(path)
                    .to_string();
                let description = op
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let raw_key = op
                    .get("operationId")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&name);
                let key = sanitize(raw_key);
                let key = if key.is_empty() {
                    sanitize(&format!("{method}-{path}"))
                } else {
                    key
                };

                let mut headers = Vec::new();
                let mut query = Vec::new();
                let mut body = storage::Body::default();

                if let Some(params) = op.get("parameters").and_then(|p| p.as_array()) {
                    for p in params {
                        let Some(p) = p.as_object() else { continue };
                        let Some(loc) = p.get("in").and_then(|v| v.as_str()) else { continue };
                        let k = p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if k.is_empty() {
                            continue;
                        }
                        let mut val = p
                            .get("schema")
                            .and_then(|s| s.get("default"))
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string();
                        if val.is_empty() {
                            val = p
                                .get("example")
                                .and_then(|e| e.as_str())
                                .unwrap_or("")
                                .to_string();
                        }
                        match loc {
                            "header" => headers.push(storage::KeyValue { key: k, value: val, enabled: true }),
                            "query" => query.push(storage::KeyValue { key: k, value: val, enabled: true }),
                            _ => {}
                        }
                    }
                }

                if let Some(rb) = op.get("requestBody") {
                    let Some(content) = rb.get("content").and_then(|c| c.as_object()) else {
                        continue;
                    };
                    if let Some(json_media) = content.get("application/json").and_then(|m| m.as_object()) {
                        let example: Option<Value> = json_media
                            .get("example")
                            .cloned()
                            .or_else(|| {
                                json_media
                                    .get("examples")
                                    .and_then(|e| e.as_object())
                                    .and_then(|m| m.values().next())
                                    .and_then(|x| x.get("value"))
                                    .cloned()
                            })
                            .or_else(|| json_media.get("schema").cloned());
                        let content_str = match example {
                            Some(v) => serde_json::to_string_pretty(&v).map_err(|_| "序列化示例失败").unwrap_or("{}".into()),
                            None => "{}".into(),
                        };
                        body = storage::Body {
                            mode: "json".into(),
                            content: content_str,
                            content_type: "application/json".into(),
                            ..Default::default()
                        };
                    } else if let Some(text_media) = content.get("text/plain").and_then(|m| m.as_object()) {
                        let content_str = text_media
                            .get("example")
                            .and_then(|e| e.as_str())
                            .unwrap_or("")
                            .to_string();
                        body = storage::Body {
                            mode: "raw".into(),
                            content: content_str,
                            content_type: "text/plain".into(),
                            ..Default::default()
                        };
                    }
                }

                let mut auth = storage::Auth::default();
                if let Some(kind) = &security_required {
                    match kind.as_str() {
                        "bearer" => {
                            auth = storage::Auth { kind: "bearer".into(), token: "{{token}}".into(), ..Default::default() };
                        }
                        "basic" => {
                            auth = storage::Auth {
                                kind: "basic".into(),
                                username: "{{username}}".into(),
                                password: "{{password}}".into(),
                                ..Default::default()
                            };
                        }
                        "api-key" => {
                            auth = storage::Auth {
                                kind: "api-key".into(),
                                api_key_name: "X-API-Key".into(),
                                api_key_in: "header".into(),
                                api_key_value: "{{api_key}}".into(),
                                ..Default::default()
                            };
                        }
                        _ => {}
                    }
                }

                let url = if server_base.is_empty() {
                    path.to_string()
                } else {
                    format!("{server_base}{path}")
                };

                let group_path: Vec<String> = op
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|t| t.as_str())
                    .map(|t| {
                        let k = sanitize(t);
                        if k.is_empty() || k == "default" {
                            Vec::new()
                        } else {
                            vec![k]
                        }
                    })
                    .unwrap_or_default();

                list.push(ImportedIface {
                    group_path,
                    key,
                    name,
                    method: method.to_uppercase(),
                    url,
                    headers,
                    query,
                    body,
                    auth,
                    description,
                });
            }
        }
    }

    if list.is_empty() {
        return Err("未解析到任何 paths 接口".into());
    }
    Ok((info_name, list))
}

/// 解析 Postman Collection v2（JSON）→ 待导入接口列表
pub fn parse_postman(content: &str) -> Result<(String, Vec<ImportedIface>), String> {
    let doc: Value = serde_json::from_str(content).map_err(|e| format!("JSON 解析失败：{e}"))?;
    let name = doc
        .get("info")
        .and_then(|i| i.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Postman 导入")
        .to_string();

    let list = walk_postman_items(doc.get("item").unwrap_or(&Value::Null), Vec::new());
    if list.is_empty() {
        return Err("未解析到任何请求".into());
    }
    Ok((name, list))
}

fn walk_postman_items(items: &Value, base: Vec<String>) -> Vec<ImportedIface> {
    let mut out = Vec::new();
    if let Some(arr) = items.as_array() {
        for item in arr {
            let item_name = item.get("name").and_then(|n| n.as_str()).unwrap_or("unnamed").to_string();
            if let Some(req) = item.get("request") {
                let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("GET").to_uppercase();
                let (url, query) = extract_postman_url(req);
                let mut headers = Vec::new();
                if let Some(hs) = req.get("header").and_then(|h| h.as_array()) {
                    for h in hs {
                        let k = h.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if k.is_empty() {
                            continue;
                        }
                        let v = h.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        headers.push(storage::KeyValue { key: k, value: v, enabled: true });
                    }
                }
                let mut body = storage::Body::default();
                if let Some(b) = req.get("body") {
                    let mode = b.get("mode").and_then(|m| m.as_str()).unwrap_or("");
                    match mode {
                        "raw" => {
                            let raw_str = b.get("raw").and_then(|r| r.as_str()).unwrap_or("").to_string();
                            body = storage::Body {
                                mode: "raw".into(),
                                content: raw_str,
                                content_type: "text/plain".into(),
                                ..Default::default()
                            };
                        }
                        "urlencoded" => {
                            let form: Vec<storage::KeyValue> = b.get("urlencoded")
                                .and_then(|u| u.as_array()).unwrap_or(&Vec::new())
                                .iter()
                                .filter_map(|x| {
                                    let key = x.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
                                    if key.is_empty() { return None; }
                                    Some(storage::KeyValue {
                                        key,
                                        value: x.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        enabled: true,
                                    })
                                })
                                .collect();
                            body = storage::Body {
                                mode: "urlencoded".into(),
                                form,
                                ..Default::default()
                            };
                        }
                        "formdata" => {
                            let form: Vec<storage::KeyValue> = b.get("formdata")
                                .and_then(|u| u.as_array()).unwrap_or(&Vec::new())
                                .iter()
                                .filter_map(|x| {
                                    let key = x.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
                                    if key.is_empty() { return None; }
                                    Some(storage::KeyValue {
                                        key,
                                        value: x.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        enabled: true,
                                    })
                                })
                                .collect();
                            body = storage::Body {
                                mode: "form-data".into(),
                                form,
                                ..Default::default()
                            };
                        }
                        "file" => {
                            body = storage::Body { mode: "file".into(), ..Default::default() };
                        }
                        _ => {}
                    }
                }
                let key = sanitize(&item_name);
                let key = if key.is_empty() {
                    sanitize(&format!("{method}-{}", base.len()))
                } else {
                    key
                };
                out.push(ImportedIface {
                    group_path: base.clone(),
                    key,
                    name: item_name.clone(),
                    method,
                    url,
                    headers,
                    query,
                    body,
                    auth: storage::Auth::default(),
                    description: String::new(),
                });
            }
            if let Some(children) = item.get("item") {
                let mut sub = base.clone();
                if !item_name.is_empty() {
                    let k = sanitize(&item_name);
                    sub.push(if k.is_empty() { format!("group-{}", sub.len()) } else { k });
                }
                out.extend(walk_postman_items(children, sub));
            }
        }
    }
    out
}

fn extract_postman_url(req: &Value) -> (String, Vec<storage::KeyValue>) {
    if let Some(r) = req.get("url").and_then(|u| u.as_str()) {
        return (r.to_string(), Vec::new());
    }
    let url_obj = req.get("url");
    let base = url_obj
        .and_then(|u| u.get("raw"))
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    let mut query = Vec::new();
    if let Some(qs) = url_obj.and_then(|u| u.get("query")).and_then(|q| q.as_array()) {
        for q in qs {
            let k = q.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if k.is_empty() { continue; }
            let v = q.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            query.push(storage::KeyValue { key: k, value: v, enabled: true });
        }
    }
    (base, query)
}

/// 把待导入接口写入目标项目
pub fn import_into_project(
    root: &Path,
    team_key: &str,
    project_key: &str,
    ifaces: &[ImportedIface],
) -> Result<ImportReport, String> {
    let mut report = ImportReport::default();
    let mut used_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for iface in ifaces {
        let mut path = Vec::new();
        for seg in &iface.group_path {
            path.push(seg.clone());
            let prefix = &path[..path.len() - 1];
            let _ = storage::create_group(root, team_key, project_key, prefix, seg, seg);
        }
    }

    for iface in ifaces {
        let mut final_key = iface.key.clone();
        let mut n = 1;
        while !used_keys.insert(final_key.clone()) {
            n += 1;
            final_key = format!("{}-{n}", iface.key);
        }
        if let Err(e) = storage::create_interface(root, team_key, project_key, &iface.group_path, &final_key, &iface.name) {
            report.skipped += 1;
            report.warnings.push(format!("接口 {} 创建失败：{e}", iface.key));
            continue;
        }
        let mut f = iface.to_file();
        f.name = iface.name.clone();
        f.method = iface.method.clone();
        f.url = iface.url.clone();
        f.headers = iface.headers.clone();
        f.query = iface.query.clone();
        f.body = iface.body.clone();
        f.auth = iface.auth.clone();
        f.description = iface.description.clone();
        match storage::save_interface(root, team_key, project_key, &iface.group_path, &final_key, &f) {
            Ok(_) => report.total += 1,
            Err(e) => {
                report.skipped += 1;
                report.warnings.push(format!("接口 {final_key} 写入失败：{e}"));
            }
        }
    }
    Ok(report)
}

pub struct ExportOutcome {
    pub content: String,
    pub warnings: Vec<String>,
}

/// 把项目的接口树导出为 OpenAPI 3.0（JSON 或 YAML）
pub fn export_openapi(root: &Path, team_key: &str, project_key: &str, want_yaml: bool) -> Result<ExportOutcome, String> {
    let settings = crate::storage::get_project_settings(root, team_key, project_key)?;
    let tree = crate::storage::list_interface_tree(root, team_key, project_key);
    let mut warnings = Vec::new();
    let mut paths: serde_json::Map<String, Value> = serde_json::Map::new();

    fn walk_nodes(
        nodes: &[crate::storage::TreeNode],
        base: Vec<String>,
        paths: &mut serde_json::Map<String, Value>,
        hosts: &mut Vec<String>,
        team: &str,
        proj: &str,
        root: &Path,
        warnings: &mut Vec<String>,
    ) {
        for node in nodes {
            match node {
                crate::storage::TreeNode::Group { key, children, .. } => {
                    let mut p = base.clone();
                    p.push(key.clone());
                    walk_nodes(children, p, paths, hosts, team, proj, root, warnings);
                }
                crate::storage::TreeNode::Interface { key, .. } => {
                    let Ok(iface) = storage::get_interface(root, team, proj, &base, key) else { continue };
                    let trimmed = iface.url.trim();
                    let (path_str, host) = split_url(trimmed);
                    if let Some(h) = &host {
                        if !hosts.contains(h) {
                            hosts.push(h.clone());
                        }
                    }
                    let path_str = if path_str.is_empty() { "/".to_string() } else { path_str };

                    let mut parameters = Vec::new();
                    for q in iface.query.iter().filter(|x| x.enabled && !x.key.is_empty()) {
                        parameters.push(json!({ "name": q.key, "in": "query", "schema": { "type": "string" } }));
                    }
                    for h in iface.headers.iter().filter(|x| x.enabled && !x.key.is_empty()) {
                        if h.key.eq_ignore_ascii_case("content-type") || h.key.eq_ignore_ascii_case("authorization") {
                            continue;
                        }
                        parameters.push(json!({ "name": h.key, "in": "header", "schema": { "type": "string" } }));
                    }

                    let mut operation = json!({
                        "operationId": format!("{}-{}", proj, key),
                        "summary": iface.name,
                        "parameters": parameters,
                        "responses": { "200": { "description": "成功" } }
                    });
                    if !iface.description.is_empty() {
                        operation["description"] = json!(iface.description);
                    }
                    match iface.body.mode.as_str() {
                        "json" => {
                            let schema = serde_json::from_str::<Value>(&iface.body.content).unwrap_or(json!({}));
                            operation["requestBody"] = json!({
                                "content": { "application/json": { "schema": schema } }
                            });
                        }
                        "raw" => {
                            let ct = iface.body.content_type.clone();
                            operation["requestBody"] = json!({
                                "content": { (ct): { "schema": { "type": "string" } } }
                            });
                        }
                        _ => {}
                    }
                    match iface.auth.kind.as_str() {
                        "bearer" => operation["security"] = json!([{"apidock_bearer": []}]),
                        "basic" => operation["security"] = json!([{"apidock_basic": []}]),
                        _ => {}
                    }

                    if trimmed.is_empty() {
                        warnings.push(format!("接口 {} 的 URL 为空", iface.name));
                    }
                    let entry = paths.entry(path_str.to_string()).or_insert_with(|| json!({}));
                    entry[iface.method.to_lowercase()] = operation;
                }
            }
        }
    }

    let mut hosts: Vec<String> = Vec::new();
    walk_nodes(&tree, Vec::new(), &mut paths, &mut hosts, team_key, project_key, root, &mut warnings);

    let mut schemes = serde_json::Map::new();
    schemes.insert("apidock_bearer".into(), json!({"type":"http","scheme":"bearer"}));
    schemes.insert("apidock_basic".into(), json!({"type":"http","scheme":"basic"}));
    let components = json!({ "securitySchemes": Value::Object(schemes) });

    let mut doc_map = serde_json::Map::new();
    doc_map.insert("openapi".into(), json!("3.0.3"));
    doc_map.insert("info".into(), json!({ "title": settings.name, "version": "1.0.0" }));
    doc_map.insert("paths".into(), Value::Object(paths));
    doc_map.insert("components".into(), components);
    if !hosts.is_empty() {
        let servers: Vec<Value> = hosts.iter().map(|h| json!({ "url": h })).collect();
        doc_map.insert("servers".into(), Value::Array(servers));
    }
    let doc = Value::Object(doc_map);

    let content = if want_yaml {
        serde_yaml_ng::to_string(&doc).map_err(|e| format!("YAML 序列化失败：{e}"))?
    } else {
        serde_json::to_string_pretty(&doc).map_err(|e| format!("JSON 序列化失败：{e}"))?
    };

    Ok(ExportOutcome { content, warnings })
}

fn split_url(url: &str) -> (String, Option<String>) {
    let url = url.trim();
    if let Some(idx) = url.find("://") {
        let after = &url[idx + 3..];
        if let Some(slash) = after.find('/') {
            let path = &after[slash..];
            (path.to_string(), Some(url[..idx + 3 + slash].to_string()))
        } else {
            (String::new(), Some(url.to_string()))
        }
    } else {
        (url.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"{
  "openapi": "3.0.3",
  "info": { "title": "用户服务", "version": "1.0" },
  "servers": [ { "url": "https://api.example.com/v1" } ],
  "components": { "securitySchemes": { "auth": { "type": "http", "scheme": "bearer" } } },
  "security": [ { "auth": [] } ],
  "paths": {
    "/users": {
      "get": {
        "tags": ["users"],
        "operationId": "listUsers",
        "summary": "用户列表",
        "parameters": [ { "name": "page", "in": "query", "schema": { "type": "integer" } } ],
        "responses": {}
      },
      "post": {
        "tags": ["users"],
        "operationId": "createUser",
        "requestBody": { "content": { "application/json": { "example": { "name": "x" } } } },
        "responses": {}
      }
    }
  }
}"#;

    #[test]
    fn openapi_parse_maps_paths_and_auth() {
        let (name, list) = parse_openapi(SPEC, false).unwrap();
        assert_eq!(name, "用户服务");
        assert_eq!(list.len(), 2);
        let get = list.iter().find(|i| i.key == "listusers").unwrap();
        assert_eq!(get.method, "GET");
        assert_eq!(get.url, "https://api.example.com/v1/users");
        assert_eq!(get.group_path, vec!["users".to_string()]);
        assert_eq!(get.auth.kind, "bearer");
        assert_eq!(get.query[0].key, "page");
        let post = list.iter().find(|i| i.key == "createuser").unwrap();
        assert_eq!(post.body.mode, "json");
        assert!(post.body.content.contains("name"));
    }

    #[test]
    fn postman_parse_nested_groups() {
        let spec = r#"{
  "info": { "name": "订单中心", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
  "item": [
    { "name": "订单", "item": [
      { "name": "创建订单", "request": { "method": "POST", "url": { "raw": "https://api.x.com/orders", "query": [ { "key": "x", "value": "1" } ] }, "body": { "mode": "raw", "raw": "{\"a\":1}" } } }
    ] },
    { "name": "删除订单", "request": { "method": "DELETE", "url": "https://api.x.com/orders/1" } }
  ]
}"#;
        let (name, list) = parse_postman(spec).unwrap();
        assert_eq!(name, "订单中心");
        assert_eq!(list.len(), 2);
        let create = list.iter().find(|i| i.name == "创建订单").unwrap();
        assert_eq!(create.group_path, vec!["group-0".to_string()]);
        assert_eq!(create.body.mode, "raw");
        assert_eq!(create.query[0].key, "x");
    }

    #[test]
    fn openapi_export_roundtrip() {
        let root = std::env::temp_dir().join(format!("apidock-exp-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        storage::ensure_root(&root).unwrap();
        storage::create_team(&root, "ops", "运维").unwrap();
        storage::create_project(&root, "ops", "p", "导出项目").unwrap();
        storage::create_interface(&root, "ops", "p", &[], "get-users", "用户列表").unwrap();
        let mut f = storage::InterfaceFile::new("get-users");
        f.url = "https://api.example.com/v1/users".into();
        f.method = "GET".into();
        storage::save_interface(&root, "ops", "p", &[], "get-users", &f).unwrap();

        let out = export_openapi(&root, "ops", "p", false).unwrap();
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["paths"]["/v1/users"]["get"]["operationId"].as_str().unwrap(), "p-get-users");

        let (_, list) = parse_openapi(&out.content, false).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].url, "https://api.example.com/v1/users");

        std::fs::remove_dir_all(&root).unwrap();
    }
}