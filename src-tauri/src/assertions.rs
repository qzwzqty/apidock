use crate::storage::Assertion;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionResult {
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonPathResult {
    pub found: bool,
    pub values: Vec<String>,
    pub first: Option<String>,
}

/// 最小 JSONPath 子集求值：
/// 支持 `$.a.b`、`.a`、`['a']`、`["a"]`、`[n]`、`[*]`、`..` 一层通配（`$.a[*].b`）
pub fn json_path(root: &Value, path: &str) -> JsonPathResult {
    let mut current: Vec<&Value> = vec![root];
    let mut tokens = tokenize(path);
    let mut first_token = true;
    while let Some(tok) = tokens.next() {
        if let Token::Root = &tok {
            if !first_token {
                break; // 中途出现 $ 视为非法
            }
            first_token = false;
            continue;
        }
        let mut next: Vec<&Value> = Vec::new();
        match tok {
            Token::Key(k) => {
                for v in &current {
                    if let Value::Object(map) = v {
                        if let Some(x) = map.get(&k) {
                            next.push(x);
                        }
                    }
                }
            }
            Token::Index(i) => {
                for v in &current {
                    if let Value::Array(arr) = v {
                        if let Some(x) = arr.get(i) {
                            next.push(x);
                        }
                    }
                }
            }
            Token::Wildcard => {
                for v in &current {
                    match v {
                        Value::Object(map) => next.extend(map.values()),
                        Value::Array(arr) => next.extend(arr.iter()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        current = next;
        first_token = false;
        if current.is_empty() {
            break;
        }
    }
    let values = current
        .into_iter()
        .map(|v| value_to_string(v))
        .collect::<Vec<_>>();
    JsonPathResult {
        found: !values.is_empty(),
        first: values.first().cloned(),
        values,
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

enum Token {
    Root,
    Key(String),
    Index(usize),
    Wildcard,
}

fn tokenize(path: &str) -> impl Iterator<Item = Token> + '_ {
    let bytes = path.as_bytes();
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            return None;
        }
        match bytes[pos] {
            b'$' => {
                pos += 1;
                Some(Token::Root)
            }
            b'.' => {
                // 可能是 `.*` 或 `.key`
                if pos + 1 < bytes.len() && bytes[pos + 1] == b'*' {
                    pos += 2;
                    Some(Token::Wildcard)
                } else if pos + 1 < bytes.len() && bytes[pos + 1] == b'.' {
                    // `..`：视为下一层通配（简化）
                    pos += 2;
                    Some(Token::Wildcard)
                } else {
                    pos += 1;
                    let start = pos;
                    while pos < bytes.len()
                        && bytes[pos].is_ascii_alphanumeric()
                        || (pos < bytes.len()
                            && matches!(bytes[pos], b'_' | b'-'))
                    {
                        pos += 1;
                    }
                    let key = path[start..pos].to_string();
                    Some(Token::Key(key))
                }
            }
            b'[' => {
                pos += 1;
                if pos < bytes.len() && bytes[pos] == b'*' {
                    pos += 2; // 跳过 `*]`
                    Some(Token::Wildcard)
                } else if pos < bytes.len() && (bytes[pos] == b'\'' || bytes[pos] == b'"') {
                    let quote = bytes[pos];
                    pos += 1;
                    let start = pos;
                    while pos < bytes.len() && bytes[pos] != quote {
                        pos += 1;
                    }
                    let key = path[start..pos].to_string();
                    if pos < bytes.len() {
                        pos += 1; // 结束引号
                    }
                    if pos < bytes.len() && bytes[pos] == b']' {
                        pos += 1;
                    }
                    Some(Token::Key(key))
                } else {
                    let start = pos;
                    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                        pos += 1;
                    }
                    let idx = path[start..pos].parse::<usize>().unwrap_or(0);
                    if pos < bytes.len() && bytes[pos] == b']' {
                        pos += 1;
                    }
                    Some(Token::Index(idx))
                }
            }
            b'*' => {
                pos += 1;
                Some(Token::Wildcard)
            }
            _ => {
                // 裸 key（如 `data.items`）：作为键
                let start = pos;
                while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_' || bytes[pos] == b'-') {
                    pos += 1;
                }
                let key = path[start..pos].to_string();
                Some(Token::Key(key))
            }
        }
    })
}

fn compare_str(op: &str, actual: &str, expected: &str) -> bool {
    match op {
        "eq" => actual == expected,
        "ne" | "neq" => actual != expected,
        "contains" => actual.contains(expected),
        "not-contains" => !actual.contains(expected),
        "gt" => num(actual) > num(expected),
        "ge" | "gte" => num(actual) >= num(expected),
        "lt" => num(actual) < num(expected),
        "le" | "lte" => num(actual) <= num(expected),
        "regex" => match regex_lite::Regex::new(expected) {
            Ok(re) => re.is_match(actual),
            Err(_) => false,
        },
        _ => actual == expected,
    }
}

fn num(s: &str) -> f64 {
    s.trim()
        .parse::<f64>()
        .unwrap_or(f64::NAN)
}

fn op_desc(op: &str) -> &'static str {
    match op {
        "eq" => "等于",
        "ne" | "neq" => "不等于",
        "contains" => "包含",
        "not-contains" => "不包含",
        "gt" => "大于",
        "ge" | "gte" => "大于等于",
        "lt" => "小于",
        "le" | "lte" => "小于等于",
        "regex" => "匹配正则",
        _ => "等于",
    }
}

/// 对响应求值断言，逐个返回结果
pub fn check(
    status: u16,
    time_ms: u64,
    headers: &[crate::storage::KeyValue],
    body: &str,
    assertions: &[Assertion],
) -> Vec<AssertionResult> {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let mut results = Vec::new();
    for a in assertions {
        match a {
            Assertion::StatusCode { op, expected } => {
                let actual = status.to_string();
                let ok = compare_str(op, &actual, &expected.to_string());
                results.push(AssertionResult {
                    passed: ok,
                    message: format!("状态码 {} {} {}（实际 {}）", actual, op_desc(op), expected, if ok { "✓" } else { "✗" }),
                });
            }
            Assertion::Time { op, expected_ms } => {
                let ok = compare_str(op, &time_ms.to_string(), &expected_ms.to_string());
                results.push(AssertionResult {
                    passed: ok,
                    message: format!("耗时 {} ms {} {} ms（实际 {}）", time_ms, op_desc(op), expected_ms, if ok { "✓" } else { "✗" }),
                });
            }
            Assertion::Header { key, op, expected } => {
                let actual = headers
                    .iter()
                    .find(|h| h.key.eq_ignore_ascii_case(key))
                    .map(|h| h.value.clone())
                    .unwrap_or_default();
                let ok = compare_str(op, &actual, expected);
                results.push(AssertionResult {
                    passed: ok,
                    message: format!("响应头 {} {} {}（实际 {:?}）", key, op_desc(op), expected, trunc(&actual)),
                });
            }
            Assertion::JsonPath { path, op, expected } => {
                match &parsed {
                    Some(root) => {
                        let res = json_path(root, path);
                        let actual = if res.found {
                            res.values.join(",")
                        } else {
                            "__NOT_FOUND__".to_string()
                        };
                        let ok = if res.found {
                            compare_str(op, &actual, expected)
                        } else {
                            *op == "ne" || *op == "neq"
                        };
                        results.push(AssertionResult {
                            passed: ok,
                            message: format!("JSONPath {{{}}} {} {}（实际 {}{}）", path, op_desc(op), expected, trunc(&actual), if res.found { "" } else { "，路径不存在" }),
                        });
                    }
                    None => {
                        results.push(AssertionResult {
                            passed: false,
                            message: format!("响应体不是合法 JSON，无法评估 JSONPath {{{}}}", path),
                        });
                    }
                }
            }
        }
    }
    results
}

fn trunc(s: &String) -> String {
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(60).collect::<String>())
    } else {
        s.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Assertion;

    fn kv(key: &str, value: &str) -> crate::storage::KeyValue {
        crate::storage::KeyValue { key: key.into(), value: value.into(), enabled: true }
    }

    #[test]
    fn jsonpath_subset() {
        let root: Value = serde_json::json!({
            "data": { "list": [ {"id": 1, "name": "a"}, {"id": 2, "name": "b"} ] },
            "tags": ["x", "y"],
            "nested": { "a": { "b": "leaf" } }
        });
        assert_eq!(json_path(&root, "$.data.list[0].id").first.as_deref(), Some("1"));
        assert_eq!(json_path(&root, "$.data.list[*].name").values, vec!["a", "b"]);
        assert_eq!(json_path(&root, "$.tags[1]").first.as_deref(), Some("y"));
        assert_eq!(json_path(&root, "$.nested['a']['b']").first.as_deref(), Some("leaf"));
        assert_eq!(json_path(&root, "$.data.list[0].id").values, vec!["1"]);
        assert_eq!(json_path(&root, "$.data.list[0].id").found, true);
        assert_eq!(json_path(&root, "$.nonexist").found, false);
    }

    #[test]
    fn assertions_evaluate() {
        let body = r#"{"code":0,"msg":"success","items":[{"id":1}]}"#;
        let headers = vec![kv("content-type", "application/json")];
        let res = check(
            200,
            150,
            &headers,
            body,
            &[
                Assertion::StatusCode { op: "eq".into(), expected: 200 },
                Assertion::StatusCode { op: "ne".into(), expected: 500 },
                Assertion::Time { op: "lt".into(), expected_ms: 1000 },
                Assertion::Header { key: "Content-Type".into(), op: "contains".into(), expected: "json".into() },
                Assertion::JsonPath { path: "$.code".into(), op: "eq".into(), expected: "0".into() },
                Assertion::JsonPath { path: "$.items[0].id".into(), op: "eq".into(), expected: "1".into() },
                Assertion::JsonPath { path: "$.nope".into(), op: "eq".into(), expected: "1".into() },
            ],
        );
        assert_eq!(res.len(), 7);
        assert!(res.iter().take(6).all(|r| r.passed));
        assert!(!res[6].passed);
        assert!(res[5].message.contains("1"));
    }
}