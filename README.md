# Apidock

纯离线的 API 管理工具 —— 对标 Apifox / Apipost / Postman 的核心能力，用 **Rust** 构建，数据全部保存在本地，无需账号、无需服务端、无需联网。

## 功能

### 接口管理
- 以 **团队 → 项目 → 分组 → 接口** 的树形结构组织 API 定义，分组支持多级嵌套。
- 接口支持完整的请求定义：HTTP 方法、URL、请求头、查询参数、接口说明（Markdown）。
- 请求体支持 6 种模式：`none`、`json`、`raw`、`urlencoded`、`form-data`、`file`。
- **响应定义**：为接口声明返回的状态码（支持 `200` / `2XX` / `default`）、说明、Media Type 与 JSON 响应结构树。

### 接口测试
- 对任意接口执行**真实 HTTP 请求**，查看状态码、响应头、响应体（JSON 树形预览、原始/格式化切换、耗时与体积统计）。
- 发送选项：超时时间、重定向策略、代理（无 / 系统 / 自定义）、TLS 校验开关与自定义 CA 证书。
- 鉴权方案：`none`、Bearer Token、Basic、API Key、Digest。
- 请求历史记录，随时回看历史请求与响应详情。

### 环境与变量
- 项目级环境，默认内置**正式 / 测试 / 开发**三套，可删改并支持自定义。
- `{{变量名}}` 模板替换，作用于 URL、请求头、请求体与鉴权字段。
- 变量优先级：**接口级 > 项目全局变量 > 环境变量**。
- 项目全局参数：向每个请求注入 Header / Cookie / Query / Body。

### 断言与一键运行
- 断言类型：状态码、JSONPath 取值比较、响应头取值比较。
- 一键运行项目或分组下的全部接口，生成通过 / 失败汇总报告，失败接口一键跳转。

### 导入 / 导出
- 导入 OpenAPI 3.0 / 3.1（JSON / YAML）与 Postman Collection 2.x。
- 以项目为单位导出 OpenAPI 3.0 / 3.1 文档（JSON / YAML）。

### 离线优先
- 数据存于本地 SQLite 数据库，单文件 `apidock.db`，复制即可备份 / 共享。
- 无账号体系、无遥测、无自动更新、无任何网络请求。

## 开发

```bash
npm install        # 安装前端依赖
npm run tauri dev  # 开发运行
cargo test         # 运行后端测试（src-tauri 下）
```

## 打包

```bash
npm run tauri build
```

生成 **NSIS 安装程序**：

```
src-tauri/target/release/bundle/nsis/Apidock_x.x.x_x64-setup.exe
```

同时产出绿色版可执行文件 `src-tauri/target/release/apidock.exe`，可直接分发运行。

### 前提条件

- Node.js + npm（前端构建）
- Rust 工具链（cargo，后端编译）
- 目标机器需有 **WebView2 Runtime**（Win10/11 一般已内置；缺失时安装包会引导下载）

### 分发说明

- 安装包**未签名**，首次运行时 SmartScreen 可能提示，选择「更多信息 → 仍要运行」即可。
- 版本号在 `src-tauri/tauri.conf.json` 的 `version` 字段中修改，打包后同步到安装包文件名。
- 如需完全离线的安装包（WebView2 缺失时不联网下载），可在 `tauri.conf.json` 的 `bundle.windows.webviewInstallMode` 中改为 `embedBootstrapper` 或 `offlineInstaller` 后重新打包。