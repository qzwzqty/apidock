# ADR-0003: HTTP 客户端采用 reqwest + rustls，禁用 OpenSSL

**状态**: accepted

请求测试的核心是 `reqwest 0.13`，配置 `default-features = false`，显式开启 `rustls`、`http2`、`system-proxy`、`multipart`、`json`、`gzip` 等特性，运行时基于 tokio。TLS 统一走 rustls（自带 webpki-roots + 用户可追加自定义 CA），不引入 OpenSSL。

选择 rustls 的理由：纯 Rust 实现、无系统级原生依赖，利于离线打包与跨平台一致分发；OpenSSL 在 Windows 上引入额外原生库、构建与分发成本高，且内网证书场景需要自定义 CA 挂接，rustls 的 `RootCertStore` 追加导入更直接。

**Considered Options**:
- 直接使用 hyper：控制力最强但需自实现重定向/超时/代理/连接复用，触及基础设施细节过多。
- reqwest + native-tls(OpenSSL/SChannel)：依赖系统原生 TLS 栈，出现差异时难以离线复现。

**Consequences**:
- 内网上中常见"自签证书/内网 CA"，通过「追加 CA + 临时跳过校验(风险提示)」两条路径覆盖；跳过校验仅对单个请求/会话生效，不写死全局。
- 若未来需要连接互不信任的中继（如内部代理），`system-proxy` 特性已覆盖，无需切换 TLS 栈。