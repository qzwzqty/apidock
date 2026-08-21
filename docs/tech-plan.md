# Apidock 技术方案

> 版本：1.0（初稿）
> 状态：待评审
> 关联文档：`docs/requirements.md`；决策记录见 `docs/adr/`

## 1. 技术栈总览

| 层面 | 选型 | 说明 |
|---|---|---|
| 语言 | Rust（stable，edition 2024） | 后端唯一语言 |
| 桌面框架 | Tauri 2 | Rust core + 系统 WebView（Windows 为 WebView2） |
| 前端 | React + Vite + TypeScript | UI / 交互层 |
| 前端样式/组件 | Tailwind CSS + shadcn/ui（Radix 原语） | 紧凑深色桌面风、按需打包、可完全自定义 |
| 代码编辑器 | Monaco Editor | 请求体 JSON/raw 编辑、语法高亮 |
| HTTP 客户端 | reqwest 0.13 | `default-features=false, features=["rustls","http2","system-proxy","multipart","json","charset","gzip/br"]` |
| 异步运行时 | tokio | 请求执行、数据库访问 |
| 序列化 | serde / serde_json | 领域结构与请求；嵌套结构以 JSON 文本列入库 |
| YAML | serde_yaml_ng | OpenAPI YAML 解析（serde_yaml 已停更，优先维护中的 fork） |
| 数据库 | sea-orm 1.x + sea-orm-migration（sqlx SQLite 后端） | 数据根目录内单一 `apidock.db`；迁移框架管理表结构（ADR-0004） |
| OpenAPI 模型 | openapiv3 2.x（3.0）；oas3 0.2x（3.1） | 导入导出双向 |
| 前端状态 | zustand（轻量） | 树、当前请求、响应状态 |
| 数据校验（可选） | schemars | 为导出 OpenAPI 补全 schema |
| 脚本沙箱（P2） | rquickjs / boa_engine | 预留，不进入 MVP |

## 2. 架构

```
┌────────────────────────── 前端 (React + Vite + TS) ──────────────────────────┐
│ 树导航 │ 请求表单 │ Monaco 编辑区 │ 响应预览(JSON 树) │ 环境面板 │ 导入导出  │
└───────────────┬──────────────────────────────────────────────────────────┬──┘
                │ invoke (命令 + 事件)                                      │ 前端不直连 HTTP
┌───────────────▼──────────────────────────────────────────────────────────▼──┐
│                              Rust (Tauri core)                              │
│  ┌───────────┐  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐   │
│  │ db        │  │   http       │  │  variables   │  │ import / export   │   │
│  │ sea-orm   │─▶│ reqwest client│  │ {{var}} 替换 │  │ openapiv3 / oas3  │   │
│  │ 实体/迁移 │  │ TLS/代理/超时 │  │ JSONPath 提取│  │ postman 导入      │   │
│  │ 仓储/事务 │  └──────────────┘  └──────────────┘  └───────────────────┘   │
│  └───────────┘                                                              │
└──────────────────────────┬───────────────────────────────────────────────────┘
                           ▼
                数据根目录（apidock.db，SQLite / WAL）
```

要点：
- **所有网络请求在 Rust 侧执行**，前端不持有任何 HTTP 能力，保证 TLS 策略、代理、证书导入统一收敛在 `http` 模块。
- UI（egui 无需，这里指 React）与 HTTP 通过 async command 交互：`invoke("send_request", payload)` 返回响应数据；长任务用事件推送进度/取消。
- 数据只经由命令层读写 SQLite，写操作返回值即最新状态，前端据此刷新（不再有外部文件变更监听）。

## 3. 模块划分（Rust）

```
src-tauri/src/
├── main.rs / lib.rs         # Tauri 入口、managed state、invoke 注册
├── domain.rs                # 纯数据结构（无 IO）：Team/Project/Group/Interface/
│                            #   Environment/GlobalParams/Variable/Auth/Body/Assertion
├── db/                      # SQLite 持久化：实体、sea-orm-migration 迁移、仓储（事务/唯一约束）
├── legacy.rs                # 旧版文件数据一次性导入（JSONC 宽容解析仅存于此）
├── http/                    # reqwest 构建、TLS/CA/代理/超时/重定向、请求执行与取消
├── variables/               # {{var}} 模板解析与替换、JSONPath 实现（可选引入 jsonpath 库）
├── importer/                # OpenAPI 3.0/3.1、Postman Collection v2 → domain
├── exporter/                # domain → OpenAPI 3.0/3.1（YAML/JSON）
└── cmd/                     # tauri command 薄封装，参数/返回值 serde 类型
```

## 4. 关键设计决策摘要

1. **文件即真相**：磁盘文件是唯一数据源，应用内不维护独立数据库；内存中只缓存扫描结果。任何修改立即原子落盘。
2. **接口模型独立于文件名**：接口文件内含 `id` 与 `name` 字段，文件名仅当人类可读的键。重命名（改 `name`）不触发磁盘移动，避免 git 历史混乱；移动/拖拽才改文件名路径。
3. **变量替换优先级**：接口级 > 项目全局变量 > 环境变量；`host` 取自当前激活环境。对模板的解析在发送前做一次，未解析变量可阻止发送。
4. **全局参数注入**：项目级全局参数（Header/Cookie/Query/Body）在每个请求发送前按规则注入，可被请求定义覆盖。
5. **HTTP 会话**：每个工作区一个可配置的 `reqwest::Client`（超时/TLS 统一），每次发送用该 client 重建 `RequestBuilder`；Cookie 开关控制是否共用 cookie store。
6. **TLS**：采用 rustls（避开 OpenSSL，利于离线与跨平台分发、体积更小）；支持内建 CA + 用户导入 CA 追加 + 一键跳过校验（标记为危险但可回滚）。
7. **导入导出为纯函数**：importer/exporter 输入字节与选项，输出 domain 对象或字节，与存储/HTTP 解耦，便于单测与后续 CLI 化。
8. **单一窗口多标签**：标签栏状态（打开的项目 + 当前激活标签）持久化在 `workspace.json`，退出还原、启动恢复；「主窗口」标签不可关闭。

## 5. 前端结构

```
src/
├── App.tsx                  # 布局：顶部标签栏 + 视图容器（主窗口 / 项目页）
├── lib/
│   ├── workspace.ts         # 前端状态模型 + zustand store（团队/项目/标签栏）
│   ├── tree.ts              # 树节点派生（分组/接口树）
│   ├── monaco.ts            # Monaco 主题与 JSON 配置
│   └── json.ts              # 格式化 / 校验 / JSON 树渲染准备
├── components/
│   ├── MainWindow/          # 主窗口：左侧团队列表、右侧项目列表
│   ├── TabBar/              # 顶部标签栏（主窗口标签 + 项目标签）
│   ├── ProjectPage/         # 项目主页 + 项目内的接口树 + 接口多标签
│   ├── RequestForm/         # URL 栏、方法选择、Tab：params/body/headers/auth/assert
│   ├── BodyEditor/
│   ├── ResponseView/        # 状态/头/耗时/体积 + JSON 树
│   ├── EnvironmentPanel/
│   ├── ImportExportDialog/
│   └── common/              # 图标、下拉、确认框
└── styles/
```

## 6. 里程碑与工作量预估

| 里程碑 | 内容 | 预估 |
|---|---|---|
| M0 | Tauri 工程 + 前后端打通 + 目录树渲染 + 工作区入口 | 3–5 天 |
| M1 | 文件存储完整 CRUD + 原子写 + notify 刷新 | 1–2 周 |
| M2 | 发送请求全模式 + 变量替换 + TLS 选项 + 响应预览 | 2–3 周 |
| M3 | OpenAPI 导入/导出 + 导入报告 + Windows 打包与 WebView2 引导 | 2 周 |
| M4 | 断言/集合运行/JSONPath/Postman 导入/Cookie/历史 | 2–3 周 |

单人多日（0.5 FTE）约 10–14 周到达 M4。

## 7. 依赖清单（MVP 重点锁定）

```
devDependencies / frontend: react, react-dom, vite, typescript, @tauri-apps/api,
  monaco-editor, zustand, @tanstack/react-virtual (大列表), lucide-react (图标),
  tailwindcss, @tailwindcss/vite, class-variance-authority, clsx, tailwind-merge,
  shadcn/ui（radix-ui 原语）

Rust:
  tauri 2, tauri-build 2
  reqwest (rustls, http2, system-proxy, multipart, json, gzip)
  tokio (rt-multi-thread, macros, fs)
  serde, serde_json, json_comments (JSONC 读取), serde_yaml_ng, uuid (v4)
  notify
  openapiv3, oas3
  dirs (默认数据根目录), anyhow (错误), chrono (时间戳), jsonpath_lib (可选)
```

## 8. 风险与缓解

| 风险 | 缓解 |
|---|---|
| WebView2 离线分发 | 报告系统版本；捆绑离线 runtime 或引导下载离线包；macOS/Linux 用 WKWebView/WebKitGTK 无此问题 |
| 大工作区性能 | 树虚拟化、扫描去重缓存、JSON 响应虚拟化渲染 |
| 文件并发写冲突 | 单实例锁（写入时锁文件）+ 时间戳冲突提示 |
| OpenAPI 导入启发式误差 | 导入报告逐项列出 skips/warnings；保留下原始规范文件（`.apidock-src`）备查 |
| 敏感信息泄露 | logs 过滤 token；导出/报告去净化 |
| serde_yaml 维护停滞 | 使用维护中的 fork serde_yaml_ng |

## 9. 补充约定

- 命名：Rust 模块名 `snake_case`、类型 `PascalCase`、command 名 `kebab-case`；前端组件 `PascalCase`。
- 文件存储命名：数据根目录下先分**模块**目录（接口管理 = `api-mgmt/`，未来模块并列）；团队/项目/分组/接口的目录名与文件名使用英文小写 + 数字 + 连字符（无空格/特殊字符），显示名以各文件元数据 `name` 字段为准，新建时自动生成唯一英文键（可改）。
- 日志：本地文件 `logs/apidock-YYYY-MM-DD.log`（轮转），级别可配，默认 info。
- 错误：Rust command 返回统一 `Result<T, CommandError>`，前端按错误码渲染为可读中文提示。
- 版本：`workspace.json`/请求文件均含 `version` 字段，升级靠 `storage/upgrade` 迁移钩子。Apidock Schema 版本 `1` 起步。