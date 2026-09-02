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

/// 归一化常见的不规范写法：`type: [x, "null"]`（或 `[x]`）→ 单值 `type: x` + `nullable: true`。
/// openapiv3 的 Schema 只接受字符串 type，此转换在交给库解析前完成。
fn normalize_type_arrays(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            if let Some(Value::Array(arr)) = map.get("type") {
                let mut primary: Option<String> = None;
                let mut nullable = false;
                for item in arr {
                    match item {
                        Value::String(s) if s == "null" => nullable = true,
                        Value::String(s) if primary.is_none() => primary = Some(s.clone()),
                        Value::Null => nullable = true,
                        _ => {}
                    }
                }
                match primary {
                    Some(t) => {
                        map.insert("type".into(), json!(t));
                    }
                    None => {
                        map.remove("type");
                    }
                }
                if nullable {
                    map.entry("nullable").or_insert_with(|| json!(true));
                }
            }
            for v in map.values_mut() {
                let mut v2 = std::mem::take(v);
                v2 = normalize_type_arrays(v2);
                *v = v2;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                let mut v2 = std::mem::take(v);
                v2 = normalize_type_arrays(v2);
                *v = v2;
            }
        }
        _ => {}
    }
    value
}

/// 解析 `#/components/schemas/...` 引用（openapiv3 模型内）
fn resolve_schema_ref<'a>(
    r: &'a openapiv3::ReferenceOr<openapiv3::Schema>,
    c: &'a openapiv3::Components,
) -> Option<&'a openapiv3::Schema> {
    match r {
        openapiv3::ReferenceOr::Item(s) => Some(s),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/schemas/")?;
            c.schemas.get(name)?.as_item()
        }
    }
}

/// 由 Schema 生成示例值（规范未提供 example/default 时按类型生成默认值）
fn schema_example(schema: &openapiv3::Schema, components: &openapiv3::Components) -> Option<Value> {
    if let Some(v) = schema
        .schema_data
        .example
        .clone()
        .or_else(|| schema.schema_data.default.clone())
    {
        return Some(v);
    }
    match &schema.schema_kind {
        openapiv3::SchemaKind::Type(t) => match t {
            openapiv3::Type::String(st) => st
                .enumeration
                .iter()
                .flatten()
                .next()
                .map(|s| json!(s))
                .or_else(|| Some(json!(""))),
            openapiv3::Type::Integer(it) => it
                .enumeration
                .iter()
                .flatten()
                .next()
                .map(|n| json!(n))
                .or_else(|| Some(json!(0))),
            openapiv3::Type::Number(nt) => nt
                .enumeration
                .iter()
                .flatten()
                .next()
                .map(|n| json!(n))
                .or_else(|| Some(json!(0))),
            openapiv3::Type::Boolean(bt) => bt
                .enumeration
                .iter()
                .flatten()
                .next()
                .map(|b| json!(b))
                .or(Some(json!(false))),
            openapiv3::Type::Object(ot) => {
                let mut obj = serde_json::Map::new();
                for (key, prop) in &ot.properties {
                    if let Some(sub) = schema_ref_example_boxed(prop, components) {
                        obj.insert(key.clone(), sub);
                    }
                }
                if obj.is_empty()
                    && let Some(openapiv3::AdditionalProperties::Schema(extra)) = &ot.additional_properties
                    && let Some(v) = schema_ref_example(extra, components)
                {
                    obj.insert("key".into(), v);
                }
                Some(Value::Object(obj))
            }
            openapiv3::Type::Array(at) => at
                .items
                .as_ref()
                .and_then(|r| schema_ref_example_boxed(r, components))
                .map(|v| json!([v]))
                .or_else(|| Some(json!([]))),
        },
        openapiv3::SchemaKind::OneOf { one_of } => {
            one_of.first().and_then(|r| schema_ref_example(r, components))
        }
        openapiv3::SchemaKind::AnyOf { any_of } => {
            any_of.first().and_then(|r| schema_ref_example(r, components))
        }
        openapiv3::SchemaKind::AllOf { all_of } => {
            let first = all_of.first().and_then(|r| schema_ref_example(r, components));
            let mut base = match first {
                Some(Value::Object(m)) => m,
                Some(v) => return Some(v),
                None => serde_json::Map::new(),
            };
            for r in all_of.iter().skip(1) {
                if let Some(Value::Object(sub)) = schema_ref_example(r, components) {
                    for (k, v) in sub {
                        base.insert(k, v);
                    }
                }
            }
            Some(Value::Object(base))
        }
        openapiv3::SchemaKind::Any(any) => {
            if let Some(v) = any.enumeration.first() {
                return Some(v.clone());
            }
            match any.typ.as_deref() {
                Some("object") => {
                    let mut obj = serde_json::Map::new();
                    for (key, prop) in &any.properties {
                        if let Some(sub) = schema_ref_example_boxed(prop, components) {
                            obj.insert(key.clone(), sub);
                        }
                    }
                    Some(Value::Object(obj))
                }
                Some("array") => any
                    .items
                    .as_ref()
                    .and_then(|r| schema_ref_example_boxed(r, components))
                    .map(|v| json!([v]))
                    .or_else(|| Some(json!([]))),
                Some("string") => Some(json!("")),
                Some("integer") | Some("number") => Some(json!(0)),
                Some("boolean") => Some(json!(false)),
                _ => None,
            }
        }
        openapiv3::SchemaKind::Not { .. } => None,
    }
}

fn schema_ref_example(
    r: &openapiv3::ReferenceOr<openapiv3::Schema>,
    components: &openapiv3::Components,
) -> Option<Value> {
    match r {
        openapiv3::ReferenceOr::Item(s) => schema_example(s, components),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/schemas/")?;
            components
                .schemas
                .get(name)?
                .as_item()
                .and_then(|s| schema_example(s, components))
        }
    }
}

fn schema_ref_example_boxed(
    r: &openapiv3::ReferenceOr<Box<openapiv3::Schema>>,
    components: &openapiv3::Components,
) -> Option<Value> {
    match r {
        openapiv3::ReferenceOr::Item(s) => schema_example(s, components),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/schemas/")?;
            components
                .schemas
                .get(name)?
                .as_item()
                .and_then(|s| schema_example(s, components))
        }
    }
}

/// JSON Schema → 请求体结构树（Apifox 式字段树），递归解析 $ref（带循环引用保护）
fn typed_schema_to_json_body(
    schema: &openapiv3::Schema,
    components: &openapiv3::Components,
) -> domain::JsonBody {
    domain::JsonBody {
        root: typed_schema_to_field(schema, components, &mut Vec::new()).unwrap_or_default(),
    }
}

fn body_leaf(
    field_type: &str,
    name: String,
    description: String,
    example: Option<Value>,
) -> domain::BodyField {
    let example = example
        .map(|v| match v {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .unwrap_or_default();
    domain::BodyField {
        field_type: field_type.into(),
        name,
        description,
        example,
        ..Default::default()
    }
}

fn typed_schema_to_field(
    schema: &openapiv3::Schema,
    components: &openapiv3::Components,
    stack: &mut Vec<String>,
) -> Option<domain::BodyField> {
    let name = schema.schema_data.title.clone().unwrap_or_default();
    let description = schema.schema_data.description.clone().unwrap_or_default();
    let example = schema
        .schema_data
        .example
        .clone()
        .or_else(|| schema.schema_data.default.clone());
    match &schema.schema_kind {
        openapiv3::SchemaKind::Type(t) => match t {
            openapiv3::Type::String(st) => Some(body_leaf(
                "string",
                name,
                description,
                example.or_else(|| st.enumeration.iter().flatten().next().map(|s| json!(s))),
            )),
            openapiv3::Type::Integer(it) => Some(body_leaf(
                "integer",
                name,
                description,
                example.or_else(|| it.enumeration.iter().flatten().next().map(|n| json!(n))),
            )),
            openapiv3::Type::Number(nt) => Some(body_leaf(
                "number",
                name,
                description,
                example.or_else(|| nt.enumeration.iter().flatten().next().map(|n| json!(n))),
            )),
            openapiv3::Type::Boolean(bt) => Some(body_leaf(
                "boolean",
                name,
                description,
                example.or_else(|| bt.enumeration.iter().flatten().next().map(|b| json!(b))),
            )),
            openapiv3::Type::Object(ot) => {
                let mut children = Vec::new();
                for (key, prop) in &ot.properties {
                    if let Some(mut f) = schema_ref_to_field_boxed(prop, components, stack) {
                        f.key = key.clone();
                        f.required = ot.required.contains(key);
                        children.push(f);
                    }
                }
                Some(domain::BodyField {
                    field_type: "object".into(),
                    name,
                    description,
                    children,
                    ..Default::default()
                })
            }
            openapiv3::Type::Array(at) => {
                let items = at
                    .items
                    .as_ref()
                    .and_then(|r| schema_ref_to_field_boxed(r, components, stack))
                    .map(Box::new);
                Some(domain::BodyField {
                    field_type: "array".into(),
                    name,
                    description,
                    items,
                    ..Default::default()
                })
            }
        },
        openapiv3::SchemaKind::OneOf { one_of } => {
            one_of.first().and_then(|r| schema_ref_to_field(r, components, stack))
        }
        openapiv3::SchemaKind::AnyOf { any_of } => {
            any_of.first().and_then(|r| schema_ref_to_field(r, components, stack))
        }
        openapiv3::SchemaKind::AllOf { all_of } => {
            let mut children = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for r in all_of {
                if let Some(f) = schema_ref_to_field(r, components, stack)
                    && f.field_type == "object"
                {
                    for c in f.children {
                        if seen.insert(c.key.clone()) {
                            children.push(c);
                        }
                    }
                }
            }
            if children.is_empty() {
                return None;
            }
            Some(domain::BodyField {
                field_type: "object".into(),
                name,
                description,
                children,
                ..Default::default()
            })
        }
        openapiv3::SchemaKind::Any(any) => {
            if let Some(v) = any.enumeration.first().cloned() {
                return Some(body_leaf(
                    any.typ.as_deref().unwrap_or("string"),
                    name,
                    description,
                    Some(v),
                ));
            }
            match any.typ.as_deref() {
                Some("object") => {
                    let mut children = Vec::new();
                    for (key, prop) in &any.properties {
                        if let Some(mut f) = schema_ref_to_field_boxed(prop, components, stack) {
                            f.key = key.clone();
                            f.required = any.required.contains(key);
                            children.push(f);
                        }
                    }
                    Some(domain::BodyField {
                        field_type: "object".into(),
                        name,
                        description,
                        children,
                        ..Default::default()
                    })
                }
                Some("array") => {
                    let items = any
                        .items
                        .as_ref()
                        .and_then(|r| schema_ref_to_field_boxed(r, components, stack))
                        .map(Box::new);
                    Some(domain::BodyField {
                        field_type: "array".into(),
                        name,
                        description,
                        items,
                        ..Default::default()
                    })
                }
                Some("null") => Some(domain::BodyField {
                    field_type: "null".into(),
                    name,
                    description,
                    ..Default::default()
                }),
                Some(t @ ("string" | "integer" | "number" | "boolean")) => {
                    Some(body_leaf(t, name, description, None))
                }
                _ => None,
            }
        }
        openapiv3::SchemaKind::Not { .. } => None,
    }
}

/// 转换 schema 引用（引用名入栈以检测循环引用）
fn schema_ref_to_field(
    r: &openapiv3::ReferenceOr<openapiv3::Schema>,
    components: &openapiv3::Components,
    stack: &mut Vec<String>,
) -> Option<domain::BodyField> {
    match r {
        openapiv3::ReferenceOr::Item(s) => typed_schema_to_field(s, components, stack),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/schemas/")?;
            if stack.iter().any(|s| s.as_str() == name) {
                return None;
            }
            let target = components.schemas.get(name)?.as_item()?;
            stack.push(name.to_string());
            let f = typed_schema_to_field(target, components, stack);
            stack.pop();
            f
        }
    }
}

fn schema_ref_to_field_boxed(
    r: &openapiv3::ReferenceOr<Box<openapiv3::Schema>>,
    components: &openapiv3::Components,
    stack: &mut Vec<String>,
) -> Option<domain::BodyField> {
    match r {
        openapiv3::ReferenceOr::Item(s) => typed_schema_to_field(s, components, stack),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/schemas/")?;
            if stack.iter().any(|s| s.as_str() == name) {
                return None;
            }
            let target = components.schemas.get(name)?.as_item()?;
            stack.push(name.to_string());
            let f = typed_schema_to_field(target, components, stack);
            stack.pop();
            f
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

/// 解析 OpenAPI 3.x（JSON 或 YAML）→ 待导入接口列表。
/// 文档解析与 $ref 解析委托给开源库 openapiv3（https://docs.rs/openapiv3），
/// 这里只保留「规范 → 接口」的转换逻辑（参数/请求体/鉴权/分组/URL）。
pub fn parse_openapi(content: &str, is_yaml: bool) -> Result<(String, Vec<ImportedIface>), String> {
    let doc: Value = if is_yaml {
        serde_yaml_ng::from_str(content).map_err(|e| format!("YAML 解析失败：{e}"))
    } else {
        serde_json::from_str(content).map_err(|e| format!("JSON 解析失败：{e}"))
    }?;
    // 归一化 `type: [x, "null"]` 等数组写法后交给 openapiv3
    let doc = normalize_type_arrays(doc);
    let spec: openapiv3::OpenAPI =
        serde_json::from_value(doc).map_err(|e| format!("OpenAPI 文档解析失败：{e}"))?;

    let info_name = spec.info.title.clone();
    let components = spec.components.clone().unwrap_or_default();

    // 鉴权方案：components.securitySchemes + 全局 security
    let mut schemes: Vec<(String, String)> = Vec::new();
    for (name, scheme) in &components.security_schemes {
        let kind = match scheme {
            openapiv3::ReferenceOr::Item(openapiv3::SecurityScheme::HTTP { scheme: s, .. }) => {
                match s.as_str() {
                    "basic" => "basic",
                    "bearer" => "bearer",
                    _ => "",
                }
            }
            openapiv3::ReferenceOr::Item(openapiv3::SecurityScheme::APIKey { .. }) => "api-key",
            _ => "",
        };
        if !kind.is_empty() {
            schemes.push((name.clone(), kind.to_string()));
        }
    }
    let mut security_required: Option<String> = None;
    if let Some(sec) = &spec.security {
        for entry in sec {
            if let Some((name, _)) = entry.iter().next()
                && let Some((_, kind)) = schemes.iter().find(|(n, _)| n == name)
            {
                security_required = Some(kind.clone());
            }
        }
    }

    let server_base = spec
        .servers
        .first()
        .map(|s| s.url.trim().trim_end_matches('/').to_string())
        .unwrap_or_default();

    let mut list: Vec<ImportedIface> = Vec::new();
    for (path, path_item_ref) in spec.paths.iter() {
        let Some(path_item) = resolve_path_item(path_item_ref) else { continue };
        for (method, op) in path_item.iter() {
            let name = op
                .summary
                .clone()
                .or_else(|| op.operation_id.clone())
                .unwrap_or_else(|| path.to_string());
            let description = op.description.clone().unwrap_or_default();
            // 文件名 = 名称（由规范中的 summary/operationId 决定），写入时再做合法性处理
            let key = name.clone();

            let mut headers = Vec::new();
            let mut query = Vec::new();
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            // path 级参数 + operation 级参数（operation 覆盖同名参数）
            for p in path_item.parameters.iter().chain(op.parameters.iter()) {
                let Some(param) = resolve_parameter(p, &components) else { continue };
                let data = param.parameter_data_ref();
                let loc = match param {
                    openapiv3::Parameter::Query { .. } => "query",
                    openapiv3::Parameter::Header { .. } => "header",
                    openapiv3::Parameter::Path { .. } => "path",
                    openapiv3::Parameter::Cookie { .. } => "cookie",
                };
                if !seen.insert(format!("{loc}:{}", data.name)) {
                    continue;
                }
                let Some(api_param) = param_to_api_param(data, &components) else { continue };
                match loc {
                    "query" => query.push(api_param),
                    "header" => headers.push(api_param),
                    _ => {}
                }
            }

            let mut body = domain::Body::default();
            if let Some(rb_ref) = &op.request_body
                && let Some(rb) = resolve_request_body(rb_ref, &components)
            {
                if let Some(media) = rb.content.get("application/json") {
                        let example = media
                            .example
                            .clone()
                            .or_else(|| {
                                media.examples.iter().next().and_then(|(_, e)| match e {
                                    openapiv3::ReferenceOr::Item(ex) => ex.value.clone(),
                                    _ => None,
                                })
                            })
                            .or_else(|| {
                                media.schema.as_ref().and_then(|rs| schema_ref_example(rs, &components))
                            });
                        let content_str = match example {
                            Some(v) => serde_json::to_string_pretty(&v)
                                .map_err(|_| "序列化示例失败".to_string())
                                .unwrap_or_else(|_| "{}".into()),
                            None => "{}".into(),
                        };
                        let mut json_tree = media
                            .schema
                            .as_ref()
                            .and_then(|rs| resolve_schema_ref(rs, &components))
                            .map(|s| typed_schema_to_json_body(s, &components))
                            .unwrap_or_default();
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
                    } else if let Some(media) = rb.content.get("text/plain") {
                        let content_str = media
                            .example
                            .clone()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_default();
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
                .tags
                .first()
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

    if list.is_empty() {
        return Err("未解析到任何 paths 接口".into());
    }
    Ok((info_name, list))
}

fn resolve_path_item(r: &openapiv3::ReferenceOr<openapiv3::PathItem>) -> Option<&openapiv3::PathItem> {
    match r {
        openapiv3::ReferenceOr::Item(pi) => Some(pi),
        // 3.1 的 pathItems 引用不在 3.0 模型内，跳过
        openapiv3::ReferenceOr::Reference { .. } => None,
    }
}

/// 解析 `#/components/parameters/...` 引用
fn resolve_parameter<'a>(
    r: &'a openapiv3::ReferenceOr<openapiv3::Parameter>,
    c: &'a openapiv3::Components,
) -> Option<&'a openapiv3::Parameter> {
    match r {
        openapiv3::ReferenceOr::Item(p) => Some(p),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/parameters/")?;
            c.parameters.get(name)?.as_item()
        }
    }
}

/// 解析 `#/components/requestBodies/...` 引用
fn resolve_request_body<'a>(
    r: &'a openapiv3::ReferenceOr<openapiv3::RequestBody>,
    c: &'a openapiv3::Components,
) -> Option<&'a openapiv3::RequestBody> {
    match r {
        openapiv3::ReferenceOr::Item(rb) => Some(rb),
        openapiv3::ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/requestBodies/")?;
            c.request_bodies.get(name)?.as_item()
        }
    }
}

/// 参数 → 本项目 ApiParam（query/header；path/cookie 由调用方过滤）
fn param_to_api_param(
    data: &openapiv3::ParameterData,
    components: &openapiv3::Components,
) -> Option<domain::ApiParam> {
    let key = data.name.clone();
    if key.is_empty() {
        return None;
    }
    let schema = match &data.format {
        openapiv3::ParameterSchemaOrContent::Schema(rs) => resolve_schema_ref(rs, components),
        _ => None,
    };
    let param_type = schema.map(schema_type_name).unwrap_or_else(|| "string".to_string());
    let example = data
        .example
        .clone()
        .or_else(|| schema.and_then(|s| schema_example(s, components)))
        .map(|v| match v {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .unwrap_or_default();
    let description = data
        .description
        .clone()
        .or_else(|| schema.and_then(|s| s.schema_data.description.clone()))
        .unwrap_or_default();
    Some(domain::ApiParam {
        key,
        example,
        required: data.required,
        param_type,
        description,
        enabled: true,
    })
}

/// Schema → 参数类型名（空/未知按 string）
fn schema_type_name(schema: &openapiv3::Schema) -> String {
    match &schema.schema_kind {
        openapiv3::SchemaKind::Type(t) => match t {
            openapiv3::Type::String(_) => "string",
            openapiv3::Type::Integer(_) => "integer",
            openapiv3::Type::Number(_) => "number",
            openapiv3::Type::Boolean(_) => "boolean",
            openapiv3::Type::Object(_) => "object",
            openapiv3::Type::Array(_) => "array",
        }
        .to_string(),
        openapiv3::SchemaKind::OneOf { one_of } => one_of
            .first()
            .and_then(|r| match r {
                openapiv3::ReferenceOr::Item(s) => Some(s),
                _ => None,
            })
            .map(schema_type_name)
            .unwrap_or_else(|| "string".into()),
        openapiv3::SchemaKind::AnyOf { any_of } => any_of
            .first()
            .and_then(|r| match r {
                openapiv3::ReferenceOr::Item(s) => Some(s),
                _ => None,
            })
            .map(schema_type_name)
            .unwrap_or_else(|| "string".into()),
        openapiv3::SchemaKind::Any(any) => any.typ.clone().unwrap_or_else(|| "string".into()),
        _ => "string".into(),
    }
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

/// 把待导入接口写入目标项目（单个事务批量落库：缺失分组自动创建，失败不残留半成品）
pub async fn import_into_project(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    ifaces: &[ImportedIface],
) -> Result<ImportReport, String> {
    let mut report = ImportReport::default();
    let mut used_names: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();

    // 先做本批内的名称去重计算
    let mut items = Vec::with_capacity(ifaces.len());
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
        let mut f = iface.to_file();
        f.name = final_name.clone();
        f.method = iface.method.clone();
        f.url = iface.url.clone();
        f.headers = iface.headers.clone();
        f.query = iface.query.clone();
        f.body = iface.body.clone();
        f.auth = iface.auth.clone();
        f.description = iface.description.clone();
        items.push(crate::db::repo::ImportItem {
            group_path: iface.group_path.clone(),
            key: final_name,
            doc: f,
        });
    }

    let results = crate::db::repo::apply_import(db, team_key, project_key, &items).await?;
    for (item, res) in items.iter().zip(results) {
        match res {
            Ok(()) => report.total += 1,
            Err(e) => {
                report.skipped += 1;
                report.warnings.push(format!("接口 {} 导入失败：{e}", item.key));
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
    let all = crate::db::repo::list_interfaces_full(db, team_key, project_key).await;
    let mut by_ref: std::collections::HashMap<(String, String), domain::InterfaceFile> =
        std::collections::HashMap::new();
    for (path, key, f) in all {
        by_ref.insert((path.join("/"), key), f);
    }
    let mut warnings = Vec::new();
    let mut paths: serde_json::Map<String, Value> = serde_json::Map::new();

    let mut hosts: Vec<String> = Vec::new();
    let mut iface_refs: Vec<(Vec<String>, String)> = Vec::new();
    flatten_ifaces(&tree, Vec::new(), &mut iface_refs);
    for (group_path, key) in iface_refs {
        let Some(iface) = by_ref.get(&(group_path.join("/"), key.clone())) else {
            continue;
        };
        let (method, path_str, operation, host) = build_interface_operation(iface, project_key, &key, &mut warnings);
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

    #[test]
    fn openapi_resolves_ref_parameters_and_type_array() {
        // 回归：$ref 参数（#/components/parameters）必须被解析；
        // 组件里含 `type: [string, "null"]` 数组写法的 schema 也必须能解析（归一化后交给 openapiv3）
        let spec = r##"{
  "openapi": "3.0.3",
  "info": { "title": "t", "version": "1" },
  "components": {
    "parameters": {
      "PlantId": { "in": "query", "name": "plant_id", "required": true, "schema": { "type": "integer", "example": 2012 } }
    },
    "schemas": {
      "Error": { "type": "object", "properties": { "detail": { "type": ["string", "null"] } } },
      "Robot": { "type": "object", "required": ["name"], "properties": { "name": { "type": "string" }, "port": { "type": "integer" } } }
    }
  },
  "paths": {
    "/robots": {
      "get": {
        "summary": "查询机器人",
        "tags": ["robot"],
        "parameters": [ { "$ref": "#/components/parameters/PlantId" } ],
        "responses": {}
      },
      "post": {
        "summary": "新增机器人",
        "tags": ["robot"],
        "requestBody": { "content": { "application/json": { "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Robot" } } } } },
        "responses": {}
      }
    }
  }
}"##;
        let (_, list) = parse_openapi(spec, false).unwrap();
        let get = list.iter().find(|i| i.name == "查询机器人").unwrap();
        assert_eq!(get.query.len(), 1);
        assert_eq!(get.query[0].key, "plant_id");
        assert_eq!(get.query[0].param_type, "integer");
        assert_eq!(get.query[0].example, "2012");
        assert!(get.query[0].required);
        let post = list.iter().find(|i| i.name == "新增机器人").unwrap();
        assert_eq!(post.body.mode, "json");
        assert_eq!(post.body.json.root.field_type, "array");
        let item = post.body.json.root.items.as_ref().unwrap();
        assert_eq!(item.field_type, "object");
        let keys: Vec<&str> = item.children.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, vec!["name", "port"]);
        assert!(item.children.iter().any(|c| c.key == "name" && c.required));
        let v: Value = serde_json::from_str(&post.body.content).unwrap();
        assert_eq!(v[0]["port"], 0);
    }
}