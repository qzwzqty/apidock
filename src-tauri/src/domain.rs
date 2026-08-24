//! 领域模型：纯数据结构与校验规则（无 IO）。
//! 持久化在 `db` 模块（SQLite + sea-orm）。

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInfo {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenTab {
    pub team_key: String,
    pub project_key: String,
}

impl OpenTab {
    pub fn id(&self) -> String {
        format!("project:{}:{}", self.team_key, self.project_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub kind: String, // system | custom | none
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceState {
    pub version: u32,
    pub open_tabs: Vec<OpenTab>,
    pub active_tab: Option<String>,
    pub proxy: ProxyConfig,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self { version: SCHEMA_VERSION, open_tabs: Vec::new(), active_tab: None, proxy: ProxyConfig::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalParams {
    pub headers: Vec<KeyValue>,
    pub cookies: Vec<KeyValue>,
    pub query: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFile {
    pub version: u32,
    pub id: String,
    pub file: String,
    pub name: String,
    pub host: String,
    pub builtin: bool,
    pub variables: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSummary {
    pub id: String,
    pub file: String,
    pub name: String,
    pub host: String,
    pub builtin: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub name: String,
    pub active_environment_id: Option<String>,
    pub global_variables: Vec<KeyValue>,
    pub global_params: GlobalParams,
}

/// 把任意字符串洗成合法键（小写字母、数字、连字符，无空格/特殊字符）
pub fn sanitize_key(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in raw.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// 用户输入名称校验：非空、长度、控制字符与路径分隔符等（键直接作为数据库唯一键）
pub fn validate_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("名称不能为空".into());
    }
    if name.chars().count() > 120 {
        return Err("名称过长（最多 120 个字符）".into());
    }
    if name == "." || name == ".." {
        return Err("非法名称".into());
    }
    let has_illegal = name.chars().any(|c| {
        c <= '\u{1f}'
            || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    });
    if has_illegal {
        return Err("名称包含特殊字符（不允许 \\ / : * ? \" < > | 及控制字符）".into());
    }
    Ok(name.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

/// 文档化参数（接口查询参数 / 请求头 / 表单字段均用此结构，Apifox 风格）。
/// 示例值字段名保持 `value`，与旧版数据兼容（旧 kv 直接映射为字符串类型参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ApiParam {
    pub key: String,
    /// 示例值：发送请求时以示例值作为实际值
    #[serde(rename = "value")]
    pub example: String,
    /// 是否必填
    pub required: bool,
    /// 参数类型：string | integer | number | boolean | object | array | file
    #[serde(rename = "type")]
    pub param_type: String,
    /// 参数说明
    pub description: String,
    /// 发送时是否启用（默认 true）
    pub enabled: bool,
}

impl ApiParam {
    /// 发送语义：启用且参数名非空
    pub fn is_active(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

/// JSON 请求体结构树节点（字段：类型 + 必填 + 示例值 + 说明 + 子字段/数组元素）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BodyField {
    pub key: String,
    /// 中文名（字段标题，用于文档展示）
    pub name: String,
    /// 是否必填
    pub required: bool,
    /// 字段类型：object | array | string | integer | number | boolean | null
    #[serde(rename = "type")]
    pub field_type: String,
    /// 示例值（仅叶子类型使用；object/array 忽略）
    pub example: String,
    /// 字段说明
    pub description: String,
    /// object 的子字段
    pub children: Vec<BodyField>,
    /// array 的元素定义
    pub items: Option<Box<BodyField>>,
}

impl BodyField {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            name: String::new(),
            required: false,
            field_type: "string".into(),
            example: String::new(),
            description: String::new(),
            children: Vec::new(),
            items: None,
        }
    }
}

/// JSON 请求体结构树：根节点与子节点同构（同一字段类型，可任意嵌套）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JsonBody {
    /// 根节点字段（key 固定为空，作为载荷本身）
    pub root: BodyField,
}

impl JsonBody {
    pub fn new_root(field_type: &str) -> Self {
        Self { root: BodyField { field_type: field_type.into(), ..BodyField::new("") } }
    }

    /// 结构树是否"无内容"：类型为空，或仅是最初始的空 object 根（未定义任何字段）
    pub fn is_empty(&self) -> bool {
        let r = &self.root;
        r.field_type.is_empty()
            || (r.field_type == "object"
                && r.name.is_empty()
                && r.example.is_empty()
                && r.description.is_empty()
                && !r.required
                && r.children.is_empty()
                && r.items.is_none())
    }
}

impl Default for JsonBody {
    fn default() -> Self {
        Self::new_root("object")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Body {
    pub mode: String, // none | json | raw | urlencoded | form-data | file
    /// raw 文本（或 XML）；json 模式下保存由结构树生成的示例载荷（兼容旧工具读取）
    pub content: String,
    pub content_type: String,
    /// 仅 json 模式使用：结构化字段树（数据文档 + 发送时的 JSON 生成源）
    pub json: JsonBody,
    /// urlencoded / form-data 表单字段（文档化参数）
    pub form: Vec<ApiParam>,
    pub file_path: Option<String>,
}

impl Body {
    /// 生成一份内部确定的 JSON 示例载荷（不做变量替换），用于预览与保存 content
    fn json_example_value(json: &JsonBody) -> Value {
        Self::field_value(&json.root)
    }

    pub fn field_value(f: &BodyField) -> Value {
        match f.field_type.as_str() {
            "object" => {
                let mut obj = serde_json::Map::new();
                for c in &f.children {
                    if !c.key.trim().is_empty() {
                        obj.insert(c.key.clone(), Self::field_value(c));
                    }
                }
                Value::Object(obj)
            }
            "array" => match &f.items {
                Some(item) => Value::Array(vec![Self::field_value(item)]),
                None => Value::Array(Vec::new()),
            },
            "integer" => parse_int(&f.example).map(Value::from).unwrap_or(Value::from(0)),
            "number" => f
                .example
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v: &f64| v.is_finite())
                .map(Value::from)
                .unwrap_or(Value::from(0)),
            "boolean" => match f.example.trim().to_lowercase().as_str() {
                "true" | "1" | "yes" | "y" | "on" => Value::Bool(true),
                "false" | "0" | "no" | "n" | "off" => Value::Bool(false),
                _ => Value::Bool(false),
            },
            "null" => Value::Null,
            _ => Value::String(f.example.clone()),
        }
    }

    /// 由结构树生成 JSON 文本（作为 content 持久化 / 发送源）
    pub fn json_example_payload(&self) -> Option<String> {
        if self.mode != "json" || self.json.is_empty() {
            return None;
        }
        serde_json::to_string_pretty(&Self::json_example_value(&self.json)).ok()
    }
}

fn parse_int(s: &str) -> Option<i64> {
    let t = s.trim();
    t.parse::<i64>().ok().or_else(|| t.parse::<f64>().ok().map(|f| f.trunc() as i64))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Auth {
    pub kind: String, // none | bearer | basic | api-key
    pub token: String,
    pub username: String,
    pub password: String,
    pub api_key_name: String,
    pub api_key_in: String, // header | query
    pub api_key_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Assertion {
    StatusCode { op: String, expected: u16 },
    Header { key: String, op: String, expected: String },
    Time { op: String, expected_ms: u64 },
    JsonPath { path: String, op: String, expected: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InterfaceFile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<ApiParam>,
    pub query: Vec<ApiParam>,
    pub body: Body,
    pub auth: Auth,
    pub variables: Vec<KeyValue>,
    pub assertions: Vec<Assertion>,
    pub description: String,
    // 发送选项（None 用默认值）
    pub timeout_ms: Option<u64>,
    pub redirect_limit: Option<u64>,
    pub tls_verify: Option<bool>,
    pub ca_cert_path: Option<String>,
}

impl InterfaceFile {
    pub fn new(key: &str) -> Self {
        Self {
            version: SCHEMA_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            name: key.to_string(),
            method: "GET".into(),
            url: String::new(),
            headers: Vec::new(),
            query: Vec::new(),
            body: Body::default(),
            auth: Auth::default(),
            variables: Vec::new(),
            assertions: Vec::new(),
            description: String::new(),
            timeout_ms: None,
            redirect_limit: None,
            tls_verify: None,
            ca_cert_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TreeNode {
    #[serde(rename_all = "camelCase")]
    Group { key: String, name: String, children: Vec<TreeNode> },
    #[serde(rename_all = "camelCase")]
    Interface { key: String, name: String, method: String },
}

/// 请求历史总结（列表用：不含完整快照与响应体）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub id: i64,
    pub team_key: String,
    pub project_key: String,
    pub project_name: String,
    pub env_id: String,
    pub env_name: String,
    pub iface_key: String,
    pub iface_name: String,
    pub method: String,
    /// 实际发送的 URL（已解析变量；失败时回落接口原始 URL）
    pub url: String,
    pub status: Option<u16>,
    pub ok: bool,
    pub time_ms: u64,
    pub created_at_ms: i64,
}

/// 请求历史完整记录：请求定义 + 环境/全局变量快照 + 响应/错误。
/// 自包含（不依赖项目/环境仍存在），可独立重发。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecord {
    pub id: i64,
    pub team_key: String,
    pub project_key: String,
    pub project_name: String,
    pub env_id: String,
    pub env_name: String,
    pub iface_key: String,
    pub iface_name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub ok: bool,
    pub time_ms: u64,
    pub created_at_ms: i64,
    /// 发送时的接口定义（完整请求，含 body/headers/query/auth）
    pub doc: InterfaceFile,
    /// 发送时的环境快照
    pub env: EnvironmentFile,
    pub global_variables: Vec<KeyValue>,
    pub global_params: GlobalParams,
    pub response: Option<crate::http::SendResponse>,
    pub error: Option<crate::http::SendErrorInfo>,
}

/// 当前 Unix 毫秒时间戳（历史记录的时间列；epoch ms 便于前端直接换算本地日期）
pub fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_key_keeps_ascii_dashes() {
        assert_eq!(sanitize_key("Order Service"), "order-service");
        assert_eq!(sanitize_key("  API--Docs  "), "api-docs");
        assert_eq!(sanitize_key("中文团队"), "");
        assert_eq!(sanitize_key("Login_v2!"), "login-v2");
    }

    #[test]
    fn validate_name_rejects_special_chars() {
        assert!(validate_name("订单服务").is_ok());
        assert!(validate_name("User API").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a\\b").is_err());
        assert!(validate_name("a:b").is_err());
        assert!(validate_name("a*b").is_err());
        assert!(validate_name("a?b").is_err());
        assert!(validate_name("a\"b").is_err());
        assert!(validate_name("a<b").is_err());
        assert!(validate_name("a>b").is_err());
        assert!(validate_name("a|b").is_err());
        assert_eq!(validate_name("a ").unwrap(), "a"); // 尾随空格会被自动去除
        assert!(validate_name("..").is_err());
        assert_eq!(validate_name("  订单服务  ").unwrap(), "订单服务");
    }

    #[test]
    fn json_body_root_field_serializes_and_reads_back() {
        // 根节点为叶子类型（非 object/array）
        let body = Body {
            mode: "json".into(),
            json: JsonBody { root: BodyField { field_type: "string".into(), example: "{{host}}/api".into(), ..BodyField::new("") } },
            ..Default::default()
        };
        assert_eq!(Body::field_value(&body.json.root), serde_json::json!("{{host}}/api"));
        let payload = body.json_example_payload().unwrap();
        assert_eq!(payload, "\"{{host}}/api\"");
        // 序列化 → 反序列化往返
        let text = serde_json::to_string(&body).unwrap();
        let back: Body = serde_json::from_str(&text).unwrap();
        assert_eq!(back.json.root.field_type, "string");
        assert_eq!(back.json.root.example, "{{host}}/api");
    }

    #[test]
    fn api_param_send_semantics() {
        let p = ApiParam { key: "  page  ".into(), example: "2".into(), enabled: true, ..Default::default() };
        assert!(p.is_active());
        let disabled = ApiParam { key: "page".into(), enabled: false, ..Default::default() };
        assert!(!disabled.is_active());
        let empty_key = ApiParam { key: "  ".into(), enabled: true, ..Default::default() };
        assert!(!empty_key.is_active());
    }
}
