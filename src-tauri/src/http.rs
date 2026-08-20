use crate::storage::{EnvironmentFile, GlobalParams, InterfaceFile, KeyValue, ProxyConfig};
use crate::variables;
use serde::Serialize;
use std::time::{Duration, Instant};

const BODY_CAP: usize = 1_000_000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_REDIRECTS: usize = 10;

#[derive(Debug, Clone)]
pub struct SendOptions {
    pub timeout_ms: Option<u64>,
    pub redirect_limit: Option<u64>,
    pub tls_verify: Option<bool>,
    pub ca_cert_path: Option<String>,
    pub proxy: ProxyConfig,
    pub cookie_jar: Option<std::sync::Arc<reqwest::cookie::Jar>>,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            redirect_limit: None,
            tls_verify: None,
            ca_cert_path: None,
            proxy: ProxyConfig::default(),
            cookie_jar: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValue>,
    pub body: String,
    pub time_ms: u64,
    pub size_bytes: usize,
    pub truncated: bool,
    pub resolved_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendErrorInfo {
    pub kind: String,
    pub message: String,
}

fn err(kind: &str, message: impl Into<String>) -> SendErrorInfo {
    SendErrorInfo { kind: kind.into(), message: message.into() }
}

pub async fn send(
    iface: &InterfaceFile,
    env: &EnvironmentFile,
    globals: &[KeyValue],
    global_params: &GlobalParams,
    opts: &SendOptions,
) -> Result<SendResponse, SendErrorInfo> {
    let vars = variables::collect_vars(iface, env, globals);

    // 未解析变量检查（URL/头/查询/文本类 body）
    let mut missing = String::new();
    let mut mark = |s: &str| {
        for k in variables::unresolved(s) {
            if vars.contains_key(&k) || missing.contains(&k) {
                continue;
            }
            // 嵌套未展开时，若其外层模板已被替换则忽略（值内可能再引用）
            if !missing.is_empty() {
                missing.push_str(", ");
            }
            missing.push_str(&k);
        }
    };
    mark(&iface.url);
    for h in iface.headers.iter().chain(global_params.headers.iter()).chain(global_params.cookies.iter()) {
        mark(&h.value);
    }
    for q in iface.query.iter().chain(global_params.query.iter()) {
        mark(&q.value);
    }
    if matches!(iface.body.mode.as_str(), "json" | "raw" | "urlencoded" | "form-data") {
        mark(&iface.body.content);
        for kv in &iface.body.form {
            mark(&kv.key);
            mark(&kv.value);
        }
    }
    if iface.auth.kind == "bearer" {
        mark(&iface.auth.token);
    }
    if !missing.is_empty() {
        return Err(err("unresolved", format!("存在未解析的变量：{missing}")));
    }

    let resolved_url = variables::substitute(&iface.url, &vars);
    let url = resolved_url.trim();
    if url.is_empty() {
        return Err(err("url", "请求地址为空"));
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(err("url", "URL 需以 http:// 或 https:// 开头（可使用 {{host}} 变量）"));
    }

    let client = {
        let mut builder = reqwest::Client::builder();

        // 超时
        let timeout_ms = opts.timeout_ms.or(iface.timeout_ms).unwrap_or(DEFAULT_TIMEOUT_MS);
        builder = builder.timeout(Duration::from_millis(timeout_ms.max(100)));

        // 重定向
        match opts.redirect_limit.or(iface.redirect_limit) {
            Some(0) => builder = builder.redirect(reqwest::redirect::Policy::none()),
            Some(n) => builder = builder.redirect(reqwest::redirect::Policy::limited(n as usize)),
            None => builder = builder.redirect(reqwest::redirect::Policy::limited(DEFAULT_MAX_REDIRECTS)),
        }

        // TLS
        if opts.tls_verify.or(iface.tls_verify) == Some(false) {
            builder = builder.danger_accept_invalid_certs(true);
        }
        if let Some(path) = opts.ca_cert_path.as_deref().or(iface.ca_cert_path.as_deref()) {
            if !path.trim().is_empty() {
                let pem = std::fs::read(path.trim())
                    .map_err(|e| err("tls", format!("读取 CA 证书 {path} 失败：{e}")))?;
                let cert = reqwest::Certificate::from_pem(&pem)
                    .map_err(|e| err("tls", format!("解析 CA 证书失败：{e}")))?;
                builder = builder.add_root_certificate(cert);
            }
        }

        // 代理：reqwest 0.13 默认会自动探测系统代理（system-proxy feature）
        //   none/未启用  -> 显式禁用
        //   system       -> 保持自动（默认）
        //   custom       -> 指定代理
        if !opts.proxy.enabled || opts.proxy.kind == "none" {
            builder = builder.no_proxy();
        } else if opts.proxy.kind == "custom" {
            if !opts.proxy.url.trim().is_empty() {
                builder = builder.proxy(
                    reqwest::Proxy::all(opts.proxy.url.trim())
                        .map_err(|e| err("proxy", format!("代理地址无效：{e}")))?,
                );
            } else {
                builder = builder.no_proxy();
            }
        }

        // Cookie 会话：全局 jar 保持会话
        if let Some(jar) = &opts.cookie_jar {
            builder = builder.cookie_provider(jar.clone());
        }

        builder.build().map_err(|e| err("http", e.to_string()))?
    };

    let method: reqwest::Method = iface
        .method
        .trim()
        .parse()
        .map_err(|_| err("http", format!("不支持的方法：{}", iface.method)))?;

    let mut req = client.request(method, url);

    // 请求头：全局参数 > 接口头（后设的覆盖）
    for kv in global_params.headers.iter().chain(iface.headers.iter()) {
        if kv.enabled && !kv.key.trim().is_empty() {
            req = req.header(
                variables::substitute(&kv.key, &vars),
                variables::substitute(&kv.value, &vars),
            );
        }
    }
    for c in &global_params.cookies {
        if c.enabled && !c.key.trim().is_empty() {
            req = req.header(
                "Cookie",
                format!(
                    "{}={}",
                    variables::substitute(&c.key, &vars),
                    variables::substitute(&c.value, &vars)
                ),
            );
        }
    }

    // 鉴权
    match iface.auth.kind.as_str() {
        "bearer" if !iface.auth.token.trim().is_empty() => {
            let token = variables::substitute(&iface.auth.token, &vars);
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        "basic" if !iface.auth.username.trim().is_empty() => {
            req = req.basic_auth(
                variables::substitute(&iface.auth.username, &vars),
                Some(variables::substitute(&iface.auth.password, &vars)),
            );
        }
        "api-key" if !iface.auth.api_key_name.trim().is_empty() => {
            let name = variables::substitute(&iface.auth.api_key_name, &vars);
            let value = variables::substitute(&iface.auth.api_key_value, &vars);
            if iface.auth.api_key_in == "query" {
                req = req.query(&[(name, value)]);
            } else {
                req = req.header(name, value);
            }
        }
        _ => {}
    }

    // 查询参数：全局 > 接口
    for (k, v) in variables::enabled_pairs(&global_params.query, &vars)
        .into_iter()
        .chain(variables::enabled_pairs(&iface.query, &vars))
    {
        req = req.query(&[(k, v)]);
    }

    // 请求体
    match iface.body.mode.as_str() {
        "json" => {
            let ct = if iface.body.content_type.trim().is_empty() {
                "application/json"
            } else {
                iface.body.content_type.trim()
            };
            req = req
                .header("Content-Type", ct)
                .body(variables::substitute(&iface.body.content, &vars));
        }
        "raw" => {
            let ct = if iface.body.content_type.trim().is_empty() {
                "text/plain"
            } else {
                iface.body.content_type.trim()
            };
            req = req
                .header("Content-Type", ct)
                .body(variables::substitute(&iface.body.content, &vars));
        }
        "urlencoded" => {
            let pairs = variables::enabled_pairs(&iface.body.form, &vars);
            req = req
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(serde_urlencoded::to_string(pairs).map_err(|e| err("http", e.to_string()))?);
        }
        "form-data" => {
            let mut form = reqwest::multipart::Form::new();
            let texts = variables::enabled_pairs(&iface.body.form, &vars);
            for (key, value) in texts {
                if let Some(path) = value.strip_prefix('@') {
                    let path = path.trim();
                    if path.is_empty() {
                        continue;
                    }
                    let data = tokio::fs::read(path)
                        .await
                        .map_err(|e| err("file", format!("读取文件 {path} 失败：{e}")))?;
                    let mime = mime_guess::from_path(path).first_or_octet_stream();
                    let part = reqwest::multipart::Part::bytes(data)
                        .mime_str(mime.as_ref())
                        .map_err(|e| err("http", e.to_string()))?;
                    form = form.part(key, part);
                } else {
                    form = form.text(key, value);
                }
            }
            req = req.multipart(form);
        }
        "file" => {
            let path = iface.body.file_path.as_deref().unwrap_or("").trim();
            if !path.is_empty() {
                let data = tokio::fs::read(path).await.map_err(|e| err("file", format!("读取文件 {path} 失败：{e}")))?;
                req = req.body(data);
            }
        }
        _ => {}
    }

    // 发送
    let start = Instant::now();
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let kind = if e.is_timeout() {
                "timeout"
            } else if e.is_connect() {
                "connect"
            } else if e.is_redirect() {
                "redirect"
            } else {
                "http"
            };
            return Err(err(kind, e.to_string()));
        }
    };
    let time_ms = start.elapsed().as_millis() as u64;

    let status = resp.status();
    let status_text = format!("{status}");
    let headers: Vec<KeyValue> = resp
        .headers()
        .iter()
        .map(|(k, v)| KeyValue {
            key: k.as_str().to_string(),
            value: v.to_str().unwrap_or("<binary>").to_string(),
            enabled: true,
        })
        .collect();

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return Err(err("http", format!("读取响应体失败：{e}"))),
    };
    let size_bytes = bytes.len();
    let truncated = size_bytes > BODY_CAP;
    let slice = if truncated { &bytes[..BODY_CAP] } else { &bytes[..] };
    let body = String::from_utf8_lossy(slice).to_string();

    Ok(SendResponse {
        status: status.as_u16(),
        status_text,
        headers,
        body,
        time_ms,
        size_bytes,
        truncated,
        resolved_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Body, EnvironmentFile, InterfaceFile};

    /// 起一个一次性 HTTP 服务器线程，捕获首包请求并返回固定响应
    fn serve(
        response: &'static str,
    ) -> (std::net::SocketAddr, std::sync::mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(1) {
                let Ok(mut stream) = stream else { continue };
                use std::io::{Read, Write};
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let _ = tx.send(req);
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", response.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (addr, rx)
    }

    fn mock_env() -> EnvironmentFile {
        EnvironmentFile {
            version: 1,
            id: "env".into(),
            file: "env".into(),
            name: "e".into(),
            host: "http://nope.invalid".into(),
            builtin: false,
            variables: Vec::new(),
        }
    }

    #[tokio::test]
    async fn send_basic_json_with_substitution() {
        let (addr, rx) = serve(r#"{"ok":true}"#);
        let mut iface: InterfaceFile = InterfaceFile::new("t");
        iface.method = "POST".into();
        iface.url = format!("http://{addr}/api/{{{{tok}}}}");
        iface.headers = vec![crate::storage::KeyValue { key: "X-Token".into(), value: "{{tok}}".into(), enabled: true }];
        iface.body = Body { mode: "json".into(), content: r#"{"u":"{{tok}}"}"#.into(), content_type: String::new(), form: Vec::new(), file_path: None };
        iface.variables = vec![crate::storage::KeyValue { key: "tok".into(), value: "abc123".into(), enabled: true }];

        let res = send(&iface, &mock_env(), &[], &Default::default(), &SendOptions::default()).await.unwrap();
        assert_eq!(res.status, 200);
        assert!(res.body.contains("true"));
        assert!(res.headers.iter().any(|h| h.key == "content-type" && h.value.contains("application/json")));

        let req = rx.recv().unwrap();
        assert!(req.starts_with("POST /api/abc123"));
        assert!(req.to_lowercase().contains("x-token: abc123"));
        assert!(req.contains("\"u\":\"abc123\""));
    }

    #[tokio::test]
    async fn unresolved_variable_rejected() {
        let mut iface: InterfaceFile = InterfaceFile::new("t");
        iface.url = "http://127.0.0.1:1/{{missing}}".into();
        let err = send(&iface, &mock_env(), &[], &Default::default(), &SendOptions::default()).await.unwrap_err();
        assert_eq!(err.kind, "unresolved");
        assert!(err.message.contains("missing"));
    }

    #[tokio::test]
    async fn connection_refused_classified() {
        // 127.0.0.1 上不可达端口
        let mut iface: InterfaceFile = InterfaceFile::new("t");
        iface.url = "http://127.0.0.1:9/x".into();
        let err = send(&iface, &mock_env(), &[], &Default::default(), &SendOptions::default()).await.unwrap_err();
        assert_eq!(err.kind, "connect");
    }
}