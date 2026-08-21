use crate::domain;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use serde_json::{json, Value};

/// 从一个外部文档解析出的“待导入接口”
#[derive(Debug, Clone)]
pub struct ImportedIface {
    pub group_path: Vec<String>,
    pub key: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<domain::ApiParam>,
    pub query: Vec<domain::ApiParam>,
    pub body: domain::Body,
    pub auth: domain::Auth,
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
    pub fn to_file(&self) -> domain::InterfaceFile {
        domain::InterfaceFile::new(&self.key)
    }
}

fn sanitize(value: &str) -> String {
    crate::domain::sanitize_key(value)
}

/// 由 JSON Schema 生成示例值（规范未提供 example 时，用于填充请求体）
fn schema_example(schema: &Value, components: &Value) -> Value {
    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let path = format!("/{}", r.strip_prefix("#/").unwrap_or(r));
        if let Some(target) = components.pointer(&path) {
            return schema_example(target, components);
        }
        return Value::Null;
    }
    if let Some(ex) = schema.get("example").or_else(|| schema.get("default")) {
        return ex.clone();
    }
    if let Some(enum_v) = schema.get("enum").and_then(|e| e.as_array()).and_then(|a| a.first()) {
        return enum_v.clone();
    }
    // allOf：合并各子表的示例
    if let Some(all) = schema.get("allOf").and_then(|a| a.as_array()) {
        if let Some(first) = all.first() {
            let v = schema_example(first, components);
            if let Value::Object(mut obj) = v {
                for sub in all.iter().skip(1) {
                    if let Value::Object(sub_obj) = schema_example(sub, components) {
                        for (k, vv) in sub_obj {
                            obj.insert(k, vv);
                        }
                    }
                }
                return Value::Object(obj);
            }
            return v;
        }
    }
    if let Some(one) = schema.get("oneOf").or_else(|| schema.get("anyOf")) {
        if let Some(first) = one.as_array().and_then(|a| a.first()) {
            return schema_example(first, components);
        }
    }
    let t = schema.get("type").and_then(|v| v.as_str()).or_else(|| {
        schema
            .get("type")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
    });
    match t {
        Some("object") => {
            let mut obj = serde_json::Map::new();
            if let Some(ps) = schema.get("properties").and_then(|p| p.as_object()) {
                for (k, pschema) in ps {
                    obj.insert(k.clone(), schema_example(pschema, components));
                }
            } else if let Some(extra) = schema.get("additionalProperties") {
                if !extra.is_null() {
                    obj.insert("key".into(), schema_example(extra, components));
                }
            }
            Value::Object(obj)
        }
        Some("array") => {
            if let Some(items) = schema.get("items") {
                Value::Array(vec![schema_example(items, components)])
            } else {
                Value::Array(Vec::new())
            }
        }
        Some("string") => Value::String(String::new()),
        Some("integer") | Some("number") => Value::Number(0.into()),
        Some("boolean") => Value::Bool(false),
        Some("null") => Value::Null,
        _ => Value::Null,
    }
}

/// JSON Schema → 请求体结构树（Apifox 式字段树）
fn schema_to_json_body(schema: &Value, components: &Value) -> domain::JsonBody {
    domain::JsonBody { root: schema_to_field(schema, components).unwrap_or_default() }
}

fn schema_to_field(schema: &Value, components: &Value) -> Option<domain::BodyField> {
    let mut schema = schema.clone();
    // 解开 $ref
    while let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let path = format!("/{}", r.strip_prefix("#/").unwrap_or(r));
        let Some(target) = components.pointer(&path) else { return None };
        let mut next = target.clone();
        // 保留外层覆盖字段（description/example 等）
        if let (Some(from), Some(to)) = (schema.as_object_mut(), next.as_object_mut()) {
            for (k, v) in from.iter() {
                if k != "$ref" && !to.contains_key(k) {
                    to.insert(k.clone(), v.clone());
                }
            }
        }
        schema = next;
    }
    if schema.is_null() {
        return None;
    }
    let description = schema.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
    let title = schema.get("title").and_then(|d| d.as_str()).unwrap_or("").to_string();
    let field_type = schema
        .get("type")
        .and_then(|t| t.as_str())
        .or_else(|| schema.get("type").and_then(|t| t.as_array()).and_then(|a| a.first()).and_then(|t| t.as_str()))
        .unwrap_or("string")
        .to_string();
    match field_type.as_str() {
        "object" => {
            let mut children = Vec::new();
            if let Some(ps) = schema.get("properties").and_then(|p| p.as_object()) {
                let required = schema
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for (k, pschema) in ps {
                    if let Some(mut f) = schema_to_field(pschema, components) {
                        f.key = k.clone();
                        f.required = required.contains(k);
                        children.push(f);
                    }
                }
            }
            Some(domain::BodyField {
                field_type: "object".into(),
                name: title,
                description,
                children,
                ..Default::default()
            })
        }
        "array" => {
            let items = schema
                .get("items")
                .and_then(|i| schema_to_field(i, components))
                .map(Box::new);
            Some(domain::BodyField { field_type: "array".into(), name: title, description, items, ..Default::default() })
        }
        t => {
            let example = schema
                .get("example")
                .or_else(|| schema.get("default"))
                .or_else(|| schema.get("enum").and_then(|e| e.as_array()).and_then(|a| a.first()))
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            Some(domain::BodyField {
                field_type: t.to_string(),
                name: title,
                description,
                example,
                ..Default::default()
            })
        }
    }
}

/// 从 JSON 文本解析出结构树（导入时兜底用）
fn json_value_to_body(content_str: &str) -> domain::JsonBody {
    let Ok(v) = serde_json::from_str::<Value>(content_str) else {
        return domain::JsonBody::default();
    };
    domain::JsonBody { root: value_to_field(&v).unwrap_or_default() }
}

fn value_to_field(v: &Value) -> Option<domain::BodyField> {
    if v.is_null() {
        return Some(domain::BodyField { field_type: "null".into(), ..Default::default() });
    }
    if let Some(obj) = v.as_object() {
        let mut children = Vec::new();
        for (k, sub) in obj {
            let mut f = value_to_field(sub).unwrap_or_else(|| domain::BodyField::new(k));
            f.key = k.clone();
            children.push(f);
        }
        return Some(domain::BodyField { field_type: "object".into(), children, ..Default::default() });
    }
    if let Some(arr) = v.as_array() {
        return Some(domain::BodyField {
            field_type: "array".into(),
            items: arr.first().and_then(value_to_field).map(Box::new),
            ..Default::default()
        });
    }
    let field_type = match v {
        Value::String(_) => "string",
        Value::Number(n) => {
            if n.as_i64().is_some() {
                "integer"
            } else {
                "number"
            }
        }
        Value::Bool(_) => "boolean",
        _ => "string",
    };
    let example = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    Some(domain::BodyField { field_type: field_type.into(), example, ..Default::default() })
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
                // 文件名 = 名称（由规范中的 summary/operationId 决定），写入时再做合法性处理
                let key = name.clone();

                let mut headers = Vec::new();
                let mut query = Vec::new();
                let mut body = domain::Body::default();

                if let Some(params) = op.get("parameters").and_then(|p| p.as_array()) {
                    for p in params {
                        let Some(p) = p.as_object() else { continue };
                        let Some(loc) = p.get("in").and_then(|v| v.as_str()) else { continue };
                        let k = p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if k.is_empty() {
                            continue;
                        }
                        let schema = p.get("schema");
                        let param_type = schema
                            .and_then(|s| s.get("type"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("string")
                            .to_string();
                        let required = p.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
                        let mut example = p
                            .get("example")
                            .cloned()
                            .or_else(|| schema.and_then(|s| s.get("example")).cloned())
                            .or_else(|| schema.and_then(|s| s.get("default")).cloned());
                        if example.is_none() {
                            if let Some(t) = p.get("schema").and_then(|s| s.get("type")).and_then(|t| t.as_str()) {
                                example = match t {
                                    "integer" | "number" => Some(json!(0)),
                                    "boolean" => Some(json!(false)),
                                    "array" => Some(json!([])),
                                    "object" => Some(json!({})),
                                    _ => None,
                                };
                            }
                        }
                        let example_str = example
                            .map(|v| match v {
                                Value::String(s) => s,
                                other => other.to_string(),
                            })
                            .unwrap_or_default();
                        let description = p
                            .get("description")
                            .and_then(|d| d.as_str())
                            .or_else(|| schema.and_then(|s| s.get("description")).and_then(|d| d.as_str()))
                            .unwrap_or("")
                            .to_string();
                        let param = domain::ApiParam {
                            key: k,
                            example: example_str,
                            required,
                            param_type,
                            description,
                            enabled: true,
                        };
                        match loc {
                            "header" => headers.push(param),
                            "query" => query.push(param),
                            _ => {}
                        }
                    }
                }

if let Some(rb) = op.get("requestBody") {
                        let Some(content) = rb.get("content").and_then(|c| c.as_object()) else {
                            continue;
                        };
                        if let Some(json_media) = content.get("application/json").and_then(|m| m.as_object()) {
                            // 结构树：以 schema 为准
                            let mut json_tree = json_media
                                .get("schema")
                                .map(|s| schema_to_json_body(s, &doc))
                                .unwrap_or_default();
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
                                .or_else(|| {
                                    json_media
                                        .get("schema")
                                        .map(|s| schema_example(s, &doc))
                                });
                            let content_str = match example {
                                Some(v) => serde_json::to_string_pretty(&v).map_err(|_| "序列化示例失败").unwrap_or("{}".into()),
                                None => "{}".into(),
                            };
                            // 树为空时用示例回填结构（保持文档可用）
                            if json_tree.is_empty() {
                                json_tree = json_value_to_body(&content_str);
                            }
                            body = domain::Body {
                                mode: "json".into(),
                                content: content_str,
                                content_type: "application/json".into(),
                                json: json_tree,
                                ..Default::default()
                            };
                        } else if let Some(text_media) = content.get("text/plain").and_then(|m| m.as_object()) {
                            let content_str = text_media
                                .get("example")
                                .and_then(|e| e.as_str())
                                .unwrap_or("")
                                .to_string();
                            body = domain::Body {
                                mode: "raw".into(),
                                content: content_str,
                                content_type: "text/plain".into(),
                                ..Default::default()
                            };
                        }
                    }

                let mut auth = domain::Auth::default();
                if let Some(kind) = &security_required {
                    match kind.as_str() {
                        "bearer" => {
                            auth = domain::Auth { kind: "bearer".into(), token: "{{token}}".into(), ..Default::default() };
                        }
                        "basic" => {
                            auth = domain::Auth {
                                kind: "basic".into(),
                                username: "{{username}}".into(),
                                password: "{{password}}".into(),
                                ..Default::default()
                            };
                        }
                        "api-key" => {
                            auth = domain::Auth {
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
                        let description = h.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
                        headers.push(domain::ApiParam { key: k, example: v, description, enabled: true, ..Default::default() });
                    }
                }
                let mut body = domain::Body::default();
                if let Some(b) = req.get("body") {
                    let mode = b.get("mode").and_then(|m| m.as_str()).unwrap_or("");
                    match mode {
                        "raw" => {
                            let raw_str = b.get("raw").and_then(|r| r.as_str()).unwrap_or("").to_string();
                            body = domain::Body {
                                mode: "raw".into(),
                                content: raw_str,
                                content_type: "text/plain".into(),
                                ..Default::default()
                            };
                        }
                        "urlencoded" => {
                            let form: Vec<domain::ApiParam> = b.get("urlencoded")
                                .and_then(|u| u.as_array()).unwrap_or(&Vec::new())
                                .iter()
                                .filter_map(|x| {
                                    let key = x.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
                                    if key.is_empty() { return None; }
                                    Some(domain::ApiParam {
                                        key,
                                        example: x.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        enabled: true,
                                        ..Default::default()
                                    })
                                })
                                .collect();
                            body = domain::Body {
                                mode: "urlencoded".into(),
                                form,
                                ..Default::default()
                            };
                        }
                        "formdata" => {
                            let form: Vec<domain::ApiParam> = b.get("formdata")
                                .and_then(|u| u.as_array()).unwrap_or(&Vec::new())
                                .iter()
                                .filter_map(|x| {
                                    let key = x.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
                                    if key.is_empty() { return None; }
                                    Some(domain::ApiParam {
                                        key,
                                        example: x.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                        description: x.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                        enabled: true,
                                        ..Default::default()
                                    })
                                })
                                .collect();
                            body = domain::Body {
                                mode: "form-data".into(),
                                form,
                                ..Default::default()
                            };
                        }
                        "file" => {
                            body = domain::Body { mode: "file".into(), ..Default::default() };
                        }
                        _ => {}
                    }
                }
                let key = item_name.clone();
                out.push(ImportedIface {
                    group_path: base.clone(),
                    key,
                    name: item_name.clone(),
                    method,
                    url,
                    headers,
                    query,
                    body,
                    auth: domain::Auth::default(),
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

fn extract_postman_url(req: &Value) -> (String, Vec<domain::ApiParam>) {
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
            let param = domain::ApiParam {
                key: k,
                example: q.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                description: q.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                enabled: true,
                ..Default::default()
            };
            query.push(param);
        }
    }
    (base, query)
}

/// 用户输入/规范中的名称直接作为接口文件名：合法则原样使用，否则把非法字符替换为 `-`
fn import_name(raw: &str) -> String {
    if let Ok(n) = crate::domain::validate_name(raw) {
        return n;
    }
    let replaced: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c <= '\u{1f}' || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let replaced = replaced.trim_end_matches(|c| c == '.' || c == '-' || c == ' ').to_string();
    if replaced.is_empty() {
        "导入接口".into()
    } else {
        replaced
    }
}

/// 把待导入接口写入目标项目（数据库）
pub async fn import_into_project(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    ifaces: &[ImportedIface],
) -> Result<ImportReport, String> {
    let mut report = ImportReport::default();
    let mut used_names: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();

    for iface in ifaces {
        let mut path = Vec::new();
        for seg in &iface.group_path {
            path.push(seg.clone());
            let prefix = &path[..path.len() - 1];
            let _ = crate::db::repo::create_group(db, team_key, project_key, prefix, seg, seg).await;
        }
    }

    for iface in ifaces {
        let dir = iface.group_path.join("/");
        let name = import_name(&iface.name);
        let mut final_name = name.clone();
        let mut n = 1;
        while !used_names.entry(dir.clone()).or_default().insert(final_name.clone()) {
            n += 1;
            final_name = format!("{name}-{n}");
        }
        if final_name != name {
            report.warnings.push(format!("接口名称 {name} 在本目录重复，已重命名为 {final_name}"));
        }
        if let Err(e) = crate::db::repo::create_interface(db, team_key, project_key, &iface.group_path, &final_name, &final_name).await {
            report.skipped += 1;
            report.warnings.push(format!("接口 {name} 创建失败：{e}"));
            continue;
        }
        let mut f = iface.to_file();
        f.name = final_name.clone();
        f.method = iface.method.clone();
        f.url = iface.url.clone();
        f.headers = iface.headers.clone();
        f.query = iface.query.clone();
        f.body = iface.body.clone();
        f.auth = iface.auth.clone();
        f.description = iface.description.clone();
        match crate::db::repo::save_interface(db, team_key, project_key, &iface.group_path, &final_name, &f).await {
            Ok(_) => report.total += 1,
            Err(e) => {
                report.skipped += 1;
                report.warnings.push(format!("接口 {final_name} 写入失败：{e}"));
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
pub async fn export_openapi(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    want_yaml: bool,
) -> Result<ExportOutcome, String> {
    let settings = crate::db::repo::get_project_settings(db, team_key, project_key).await?;
    let tree = crate::db::repo::list_interface_tree(db, team_key, project_key).await;
    let mut warnings = Vec::new();
    let mut paths: serde_json::Map<String, Value> = serde_json::Map::new();

    let mut hosts: Vec<String> = Vec::new();
    let mut iface_refs: Vec<(Vec<String>, String)> = Vec::new();
    flatten_ifaces(&tree, Vec::new(), &mut iface_refs);
    for (group_path, key) in iface_refs {
        let Ok(iface) = crate::db::repo::get_interface(db, team_key, project_key, &group_path, &key).await
        else {
            continue;
        };
        let (method, path_str, operation, host) = build_interface_operation(&iface, project_key, &key, &mut warnings);
        if let Some(h) = &host {
            if !hosts.contains(h) {
                hosts.push(h.clone());
            }
        }
        let entry = paths.entry(path_str).or_insert_with(|| json!({}));
        entry[method] = operation;
    }

    let content = openapi_doc(&settings.name, paths, &hosts, want_yaml)?;
    Ok(ExportOutcome { content, warnings })
}

/// 把接口树展平为 (分组路径, 接口键) 列表
fn flatten_ifaces(nodes: &[domain::TreeNode], base: Vec<String>, out: &mut Vec<(Vec<String>, String)>) {
    for node in nodes {
        match node {
            domain::TreeNode::Group { key, children, .. } => {
                let mut p = base.clone();
                p.push(key.clone());
                flatten_ifaces(children, p, out);
            }
            domain::TreeNode::Interface { key, .. } => {
                out.push((base.clone(), key.clone()));
            }
        }
    }
}

/// 把单个接口导出为一个 OpenAPI 3.0 文档（仅一条路径）
pub async fn export_openapi_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    want_yaml: bool,
) -> Result<ExportOutcome, String> {
    let settings = crate::db::repo::get_project_settings(db, team_key, project_key).await?;
    let iface = crate::db::repo::get_interface(db, team_key, project_key, group_path, iface_key)
        .await
        .map_err(|e| format!("读取接口 {iface_key} 失败：{e}"))?;
    let mut warnings = Vec::new();
    let (method, path_str, operation, host) = build_interface_operation(&iface, project_key, iface_key, &mut warnings);
    let mut entry = serde_json::Map::new();
    entry.insert(method, operation);
    let mut paths = serde_json::Map::new();
    paths.insert(path_str, Value::Object(entry));
    let hosts: Vec<String> = host.into_iter().collect();
    let content = openapi_doc(&settings.name, paths, &hosts, want_yaml)?;
    Ok(ExportOutcome { content, warnings })
}

/// 单个接口 → (小写方法, 路径, OpenAPI operation, host)
fn build_interface_operation(
    iface: &crate::domain::InterfaceFile,
    proj: &str,
    key: &str,
    warnings: &mut Vec<String>,
) -> (String, String, Value, Option<String>) {
    let trimmed = iface.url.trim();
    let (path_str, host) = split_url(trimmed);
    let path_str = if path_str.is_empty() { "/".to_string() } else { path_str };

    let mut parameters = Vec::new();
    for q in iface.query.iter().filter(|x| x.is_active()) {
        let mut meta = json!({
            "name": q.key, "in": "query",
            "schema": { "type": schema_type_of(&q.param_type) }
        });
        if q.required {
            meta["required"] = json!(true);
        }
        if !q.description.is_empty() {
            meta["description"] = json!(q.description);
        }
        if !q.example.is_empty() {
            meta["example"] = json!(q.example);
        }
        parameters.push(meta);
    }
    for h in iface.headers.iter().filter(|x| x.is_active()) {
        if h.key.eq_ignore_ascii_case("content-type") || h.key.eq_ignore_ascii_case("authorization") {
            continue;
        }
        let mut meta = json!({
            "name": h.key, "in": "header",
            "schema": { "type": schema_type_of(&h.param_type) }
        });
        if h.required {
            meta["required"] = json!(true);
        }
        if !h.description.is_empty() {
            meta["description"] = json!(h.description);
        }
        if !h.example.is_empty() {
            meta["example"] = json!(h.example);
        }
        parameters.push(meta);
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
            // 结构树优先 → OpenAPI Schema；否则用旧 content 文本
            let schema = if !iface.body.json.is_empty() {
                let mut s = schema_of_json_body(&iface.body.json);
                if !iface.body.content.trim().is_empty() {
                    if let Ok(ex) = serde_json::from_str::<Value>(&iface.body.content) {
                        if !ex.is_null() {
                            s["example"] = ex;
                        }
                    }
                }
                s
            } else {
                serde_json::from_str::<Value>(&iface.body.content).unwrap_or(json!({}))
            };
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
    (iface.method.to_lowercase(), path_str, operation, host)
}

/// OpenAPI 3.0 文档组装（openapi / info / paths / components / servers）
fn openapi_doc(title: &str, paths: serde_json::Map<String, Value>, hosts: &[String], want_yaml: bool) -> Result<String, String> {
    let mut schemes = serde_json::Map::new();
    schemes.insert("apidock_bearer".into(), json!({"type":"http","scheme":"bearer"}));
    schemes.insert("apidock_basic".into(), json!({"type":"http","scheme":"basic"}));
    let components = json!({ "securitySchemes": Value::Object(schemes) });

    let mut doc_map = serde_json::Map::new();
    doc_map.insert("openapi".into(), json!("3.0.3"));
    doc_map.insert("info".into(), json!({ "title": title, "version": "1.0.0" }));
    doc_map.insert("paths".into(), Value::Object(paths));
    doc_map.insert("components".into(), components);
    if !hosts.is_empty() {
        let servers: Vec<Value> = hosts.iter().map(|h| json!({ "url": h })).collect();
        doc_map.insert("servers".into(), Value::Array(servers));
    }
    let doc = Value::Object(doc_map);

    if want_yaml {
        serde_yaml_ng::to_string(&doc).map_err(|e| format!("YAML 序列化失败：{e}"))
    } else {
        serde_json::to_string_pretty(&doc).map_err(|e| format!("JSON 序列化失败：{e}"))
    }
}

/// 参数类型 → OpenAPI schema type（空/未知按 string）
fn schema_type_of(t: &str) -> &str {
    match t {
        "integer" | "number" | "boolean" | "array" | "object" | "string" => t,
        "file" => "string",
        _ => "string",
    }
}

/// 结构树 → OpenAPI JSON Schema
fn schema_of_json_body(json: &domain::JsonBody) -> Value {
    if json.root.field_type.is_empty() {
        return json!({});
    }
    schema_of_field(&json.root).unwrap_or(json!({}))
}

fn schema_of_field(f: &domain::BodyField) -> Option<Value> {
    let mut node = match f.field_type.as_str() {
        "object" => {
            let mut props = serde_json::Map::new();
            let mut required = Vec::new();
            for c in &f.children {
                if c.key.trim().is_empty() {
                    continue;
                }
                if let Some(sub) = schema_of_field(c) {
                    props.insert(c.key.clone(), sub);
                }
                if c.required {
                    required.push(json!(c.key));
                }
            }
            let mut obj = json!({ "type": "object", "properties": props });
            if !required.is_empty() {
                obj["required"] = json!(required);
            }
            obj
        }
        "array" => {
            let mut arr = json!({ "type": "array" });
            if let Some(item) = &f.items {
                if let Some(sub) = schema_of_field(item) {
                    arr["items"] = sub;
                }
            }
            arr
        }
        "integer" | "number" | "boolean" | "null" => json!({ "type": f.field_type }),
        _ => {
            let mut s = json!({ "type": "string" });
            if !f.example.is_empty() {
                s["example"] = json!(f.example);
            }
            s
        }
    };
    if !f.description.is_empty() {
        node["description"] = json!(f.description);
    }
    if !f.name.is_empty() {
        node["title"] = json!(f.name);
    }
    Some(node)
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
        let get = list.iter().find(|i| i.name == "用户列表").unwrap();
        assert_eq!(get.key, "用户列表");
        assert_eq!(get.method, "GET");
        assert_eq!(get.url, "https://api.example.com/v1/users");
        assert_eq!(get.group_path, vec!["users".to_string()]);
        assert_eq!(get.auth.kind, "bearer");
        assert_eq!(get.query[0].key, "page");
        let post = list.iter().find(|i| i.key == "createUser").unwrap();
        assert_eq!(post.body.mode, "json");
        assert!(post.body.content.contains("name"));
    }

    #[test]
    fn openapi_schema_becomes_example_body() {
        let spec = r##"{
  "openapi": "3.0.3",
  "info": { "title": "t", "version": "1" },
  "components": { "schemas": { "Card": { "type": "object", "properties": { "id": { "type": "integer" }, "ok": { "type": "boolean", "default": true } }, "required": ["id"] } } },
  "paths": {
    "/cmd": {
      "post": {
        "summary": "下发",
        "requestBody": { "content": { "application/json": { "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Card" } } } } },
        "responses": {}
      }
    }
  }
}"##;
        let (_, list) = parse_openapi(spec, false).unwrap();
        let content = &list[0].body.content;
        // 生成的是示例值，而不是 schema 结构
        assert!(content.contains("id"));
        assert!(content.contains("ok"));
        assert!(!content.contains("required"));
        assert!(!content.contains("$ref"));
        let v: Value = serde_json::from_str(content).unwrap();
        assert_eq!(v[0]["id"], 0);
        assert_eq!(v[0]["ok"], true);
    }

    #[tokio::test]
    async fn import_names_files_by_display_name() {
        let db = crate::db::tests_support::temp_db("imp-name").await;
        crate::db::repo::create_team(&db, "ops", "运维").await.unwrap();
        crate::db::repo::create_project(&db, "ops", "默认模块", "默认模块").await.unwrap();
        let spec = r#"{
  "openapi": "3.0.3",
  "info": { "title": "默认模块", "version": "1.0" },
  "paths": {
    "/api/iot/command-v2": {
      "post": {
        "summary": "数据下发V2",
        "requestBody": { "content": { "application/json": { "schema": { "type": "array", "items": { "type": "object", "properties": { "command_type": { "type": "string" }, "payload": { "type": "object", "properties": {} }, "count": { "type": "integer" } }, "required": ["command_type", "payload"] } } } } },
        "responses": {}
      }
    }
  }
}"#;
        let (_, list) = parse_openapi(spec, false).unwrap();
        let report = import_into_project(&db, "ops", "默认模块", &list).await.unwrap();
        assert_eq!(report.total, 1);
        // 接口键 = 名称（中文）
        let f = crate::db::repo::get_interface(&db, "ops", "默认模块", &[], "数据下发V2").await.unwrap();
        assert_eq!(f.name, "数据下发V2");
        // 请求体为示例值而非 schema
        let v: Value = serde_json::from_str(&f.body.content).unwrap();
        assert_eq!(v[0]["command_type"], "");
        assert_eq!(v[0]["count"], 0);
        assert!(v[0].as_object().unwrap().contains_key("payload"));
    }

    #[tokio::test]
    async fn import_renames_duplicate_names_in_same_dir() {
        let db = crate::db::tests_support::temp_db("imp-dup").await;
        crate::db::repo::create_team(&db, "ops", "运维").await.unwrap();
        crate::db::repo::create_project(&db, "ops", "p", "P").await.unwrap();
        let spec = r#"{
  "openapi": "3.0.3",
  "info": { "title": "t", "version": "1" },
  "paths": {
    "/a": { "get": { "summary": "同名词条", "responses": {} } },
    "/b": { "get": { "summary": "同名词条", "responses": {} } }
  }
}"#;
        let (_, list) = parse_openapi(spec, false).unwrap();
        let report = import_into_project(&db, "ops", "p", &list).await.unwrap();
        assert_eq!(report.total, 2);
        assert!(report.warnings.iter().any(|w| w.contains("同名词条")));
        let keys: Vec<String> = crate::db::repo::list_interface_tree(&db, "ops", "p")
            .await
            .iter()
            .filter_map(|n| match n {
                domain::TreeNode::Interface { key, .. } => Some(key.clone()),
                _ => None,
            })
            .collect();
        assert!(keys.iter().any(|k| k == "同名词条"));
        assert!(keys.iter().any(|k| k == "同名词条-2"));
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

    #[tokio::test]
    async fn openapi_export_roundtrip() {
        let db = crate::db::tests_support::temp_db("exp").await;
        crate::db::repo::create_team(&db, "ops", "运维").await.unwrap();
        crate::db::repo::create_project(&db, "ops", "p", "导出项目").await.unwrap();
        crate::db::repo::create_interface(&db, "ops", "p", &[], "get-users", "用户列表").await.unwrap();
        let mut f = domain::InterfaceFile::new("get-users");
        f.url = "https://api.example.com/v1/users".into();
        f.method = "GET".into();
        crate::db::repo::save_interface(&db, "ops", "p", &[], "get-users", &f).await.unwrap();

        let out = export_openapi(&db, "ops", "p", false).await.unwrap();
        let v: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(v["paths"]["/v1/users"]["get"]["operationId"].as_str().unwrap(), "p-get-users");

        let (_, list) = parse_openapi(&out.content, false).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].url, "https://api.example.com/v1/users");
    }
}