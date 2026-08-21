use crate::domain::{EnvironmentFile, InterfaceFile, KeyValue};
use std::collections::BTreeMap;

/// 变量收集，优先级：接口级 > 项目全局 > 环境（host 取自环境，可被覆盖）
pub fn collect_vars(
    iface: &InterfaceFile,
    env: &EnvironmentFile,
    globals: &[KeyValue],
) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    for v in &env.variables {
        if v.enabled && !v.key.trim().is_empty() {
            vars.insert(v.key.trim().to_string(), v.value.clone());
        }
    }
    if !env.host.trim().is_empty() {
        vars.insert("host".to_string(), env.host.trim().to_string());
    }
    for v in globals {
        if v.enabled && !v.key.trim().is_empty() {
            vars.insert(v.key.trim().to_string(), v.value.clone());
        }
    }
    for v in &iface.variables {
        if v.enabled && !v.key.trim().is_empty() {
            vars.insert(v.key.trim().to_string(), v.value.clone());
        }
    }
    vars
}

/// 替换 `{{key}}` 模板，支持嵌套（最多 10 轮）
pub fn substitute(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut s = template.to_string();
    for _ in 0..10 {
        let mut changed = false;
        s = replace_once(&s, vars, &mut changed);
        if !changed {
            break;
        }
    }
    s
}

fn replace_once(s: &str, vars: &BTreeMap<String, String>, changed: &mut bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'{' && s[i..].starts_with("{{") {
            if let Some(end) = s[i + 2..].find("}}") {
                let key = s[i + 2..i + 2 + end].trim();
                if let Some(v) = vars.get(key) {
                    out.push_str(v);
                    *changed = true;
                    i += 2 + end + 2;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 找出未解析的变量名（去重、保序）
pub fn unresolved(template: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let s = template;
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'{' && s[i..].starts_with("{{") {
            if let Some(end) = s[i + 2..].find("}}") {
                let key = s[i + 2..i + 2 + end].trim().to_string();
                if !keys.contains(&key) {
                    keys.push(key);
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += s[i..].chars().next().unwrap().len_utf8();
    }
    keys
}

pub fn enabled_pairs(list: &[KeyValue], vars: &BTreeMap<String, String>) -> Vec<(String, String)> {
    list.iter()
        .filter(|kv| kv.enabled && !kv.key.trim().is_empty())
        .map(|kv| {
            (
                substitute(&kv.key, vars),
                substitute(&kv.value, vars),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{EnvironmentFile, InterfaceFile};

    fn kv(key: &str, value: &str) -> KeyValue {
        KeyValue { key: key.into(), value: value.into(), enabled: true }
    }

    #[test]
    fn substitute_basic_and_nested() {
        let vars = BTreeMap::from([
            ("host".to_string(), "https://api.example.com".to_string()),
            ("token".to_string(), "{{host}}/t".to_string()),
        ]);
        assert_eq!(substitute("{{host}}/api/login", &vars), "https://api.example.com/api/login");
        // 嵌套解析
        assert_eq!(substitute("x={{token}}", &vars), "x=https://api.example.com/t");
        // 未解析保留
        assert_eq!(substitute("{{missing}}/{{host}}", &vars), "{{missing}}/https://api.example.com");
        assert_eq!(unresolved("a={{x}},b={{y}},a={{x}}"), vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn precedence_iface_over_globals_over_env() {
        let iface = InterfaceFile { variables: vec![kv("k", "iface")], ..Default::default() };
        let env = EnvironmentFile {
            version: 1,
            id: "env".into(),
            file: "env".into(),
            name: "e".into(),
            host: "https://env-host".into(),
            builtin: false,
            variables: vec![kv("k", "env"), kv("only-env", "1")],
        };
        let globals = [kv("k", "global"), kv("only-global", "2")];
        let vars = collect_vars(&iface, &env, &globals);
        assert_eq!(vars.get("k").unwrap(), "iface");
        assert_eq!(vars.get("only-global").unwrap(), "2");
        assert_eq!(vars.get("only-env").unwrap(), "1");
        assert_eq!(vars.get("host").unwrap(), "https://env-host");
        // 接口级 host 覆盖环境 host
        let iface2 = InterfaceFile { variables: vec![kv("host", "https://oops")], ..Default::default() };
        let vars2 = collect_vars(&iface2, &env, &[]);
        assert_eq!(vars2.get("host").unwrap(), "https://oops");
    }
}