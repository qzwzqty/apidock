# Apidock

纯离线的 API 管理工具 —— 对标 Apifox / Apipost / Postman 的核心能力，用 **Rust** 构建，**本地文件 + 文件夹**即可管理一切，无需数据库、无需服务端、无需联网。

## 快速开始

> M0 工程骨架已完成：Tauri 2 + React + Vite + TS，数据根目录选择入口 + 团队/项目扫描 + 标签栏（含持久化）。

1. 阅读 [需求文档](docs/requirements.md) —— 功能和验收范围
2. 阅读 [技术方案](docs/tech-plan.md) —— 技术栈与模块设计
3. 阅读 [决策记录](docs/adr/) —— 关键选型及理由
4. 词汇表见 [CONTEXT.md](CONTEXT.md)
5. 开发运行：`npm install`，然后 `npm run tauri dev`
6. 测试：`cargo test`（src-tauri 下）

## 仓库结构

```
apidock/
├── CONTEXT.md              # 领域词汇表
├── docs/                   # 需求 / 技术方案 / 决策记录
├── src/                    # 前端 React + Vite + TS（Tailwind + shadcn 风格组件）
└── src-tauri/              # Rust core（storage 存储 / 标签状态 / 团队与项目命令）
```

## 设计要点（摘要）

- **组织结构**：数据根目录 → 模块（接口管理 `api-mgmt/`）→ 团队 → 项目 → 分组 → 接口；团队/项目/分组/接口以英文目录/文件名存储，中文显示名存元数据 `name` 字段。
- **存储**：工作区 = 一个目录；集合/分组 = 文件夹，请求 = 单个 JSONC 文件（原子写 + 外部变更监听）。
- **请求测试**：reqwest + rustls，支持超时/重定向/代理/TLS/自定义 CA；响应预览含 JSON 树。
- **变量**：`{{var}}` 模板，优先级 接口级 > 项目全局 > 环境级。
- **互操作**：导入/导出 OpenAPI 3.0/3.1（JSON/YAML），P1 支持 Postman Collection v2。
- **离线**：无账号、无遥测、无自动更新；Windows WebView2 走离线分发方案。