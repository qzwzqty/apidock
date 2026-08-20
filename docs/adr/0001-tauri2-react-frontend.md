# ADR-0001: 基于 Tauri 2 + React/Vite 构建桌面端

**状态**: accepted

产品需要一个接近 Postman 级的 UI（树形导航、Monaco 编辑器、多标签响应预览），而我们用 Rust 做全部后端逻辑。选择 Tauri 2（Rust 核心 + 系统 WebView + Web 前端 + React/Vite/TS）作为 UI 承载,使前端生态（Monaco、虚拟滚动、组件库）直接可用，开发效率远高于纯 Rust GUI。

**Considered Options**:
- egui/eframe：纯 Rust 单二进制、零 WebView 依赖最"离线"，但树/标签页/代码编辑器需全部手写，达到 Postman 交互水平的工作量显著更高。
- Slint：声明式、性能好，但生态与 Monaco 类编辑器集成远不如 Web 前端。

**Consequences**:
- Windows 平台引入 WebView2 Runtime 依赖，需提供离线分发/引导方案（见 ADR-0003 的 TLS 之外，发布侧由打包计划承载），但 Win10/11 多数已自带；离线能力通过打包与本地运行保证，不构成联网。
- 网络请求全部收敛在 Rust 侧（reqwest），前端不直连，因此 WebView 的存在不影响"离线/安全"主张。