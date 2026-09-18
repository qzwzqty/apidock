# Apidock

纯离线的 API 管理工具 —— 对标 Apifox / Apipost / Postman 的核心能力，用 **Rust** 构建，数据全部保存在本地，无需账号、无需服务端、无需联网。

## 功能

### 接口管理
- 以 **团队 → 项目 → 分组 → 接口** 的树形结构组织 API 定义，分组支持多级嵌套。
- 接口支持完整的请求定义：HTTP 方法、URL、请求头、查询参数、接口说明（Markdown）。
- 请求体支持 6 种模式：`none`、`json`、`raw`、`urlencoded`、`form-data`、`file`。
- **响应定义**：为接口声明返回的状态码（支持 `200` / `2XX` / `default`）、说明、Media Type 与 JSON 响应结构树。
- 左侧「接口管理」区块可折叠，树节点默认折叠，项目接口多时更清爽。

### 快捷请求
- 不建正式接口、不依赖环境地**直接试一次请求**，入口在接口树下方的「快捷请求」区块（同样可折叠、可分组）。
- 界面就是请求调试页，**只保留调试视图**（没有文档 / 编辑切换）；发送后内容自动保存，关掉标签再打开还是上次的内容。
- 地址栏填写**含 host 的完整地址**（如 `http://127.0.0.1:8080/api/login`），host **不继承环境变量**，也不受当前环境影响；`{{变量}}` 仍可取自项目全局变量与接口级变量，全局参数照常注入。
- **快捷请求分组**：可多级嵌套，与接口分组**相互独立**（不参与环境、导入导出与一键运行），互不影响。
- 名称仅作显示、**无唯一键**，可重名、可随时重命名；行内操作与接口一致：编辑 / 复制 / 移动 / 删除。
- 发送结果同样写入请求历史。

### 拖拽整理
- 接口、快捷请求与**分组**都能就地拖拽：拖到某行的**上半 / 下半部** = 插入到该行前 / 后（同层即排序），拖到**分组行中部** = 移入该分组末尾，拖到列表底部**虚线区** = 移到根层末尾。
- 分组可拖拽排序与嵌套，并有**环检测**（不能拖进自己的子树）；落地位置以插入线提示。
- 顺序持久化保存，重启后保持；移动接口分组会同步已打开标签页的路径。

### 接口测试
- 对任意接口执行**真实 HTTP 请求**，查看状态码、响应头、响应体（JSON 树形预览、原始/格式化切换、耗时与体积统计）。
- 发送选项：超时时间、重定向策略、代理（无 / 系统 / 自定义）、TLS 校验开关与自定义 CA 证书。
- 鉴权方案：`none`、Bearer Token、Basic、API Key、Digest。
- **请求历史**：每次发送自动记录，按日期分组（今天 / 昨天 / …），可按方法、地址、接口名与状态码搜索；详情页回看完整请求与响应，可修改后**再次发送**（改动不落库），支持单条删除与一键清空。

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

### 界面
- **多标签页**：最左侧是固定的主窗口（团队与项目列表），打开的项目各占一个标签，标签布局与代理设置自动持久化，重启恢复上次状态。
- 左侧「接口管理」与「快捷请求」两个区块的标题行可折叠，树节点默认折叠。
- **工作区设置**可配置代理：无 / 系统 / 自定义。
- 窗口内的拖拽使用前端拖放，因此已关闭 WebView2 的原生拖放（`tauri.conf.json` 的 `dragDropEnabled: false`），否则 Windows 上拖拽会被系统拦截。

### 离线优先
- 数据存于本地 SQLite 数据库，单文件 `apidock.db`，位于 `<用户主目录>/.apidock/apidock.db`（Windows 为 `C:\Users\<用户名>\.apidock\apidock.db`），关闭应用后复制该目录即可备份 / 共享。
- 无账号体系、无遥测、无自动更新、无任何网络请求。

## 开发

```bash
npm install                  # 安装前端依赖
npm run tauri dev            # 开发运行
npm run build                # 仅构建前端（tsc 类型检查 + vite 打包）
cd src-tauri && cargo test   # 运行后端测试
```

技术栈：前端 React + TypeScript + Vite + Tailwind + zustand；后端 Tauri 2 + Rust + sea-orm（SQLite / WAL）。

```
src/                          前端（components / lib）
src-tauri/src/                后端（db 存储、http、runner、assertions、imports、variables）
src-tauri/src/db/migration.rs 数据库迁移，新增表 / 列时追加一个 Mxxx
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

### 发布（GitHub Actions）

推送 `v*` 标签即自动编译、打包并发布 Release：

```bash
# 1. 先把 src-tauri/tauri.conf.json 的 version 改成 0.2.0
# 2. 提交后打标签并推送
git tag v0.2.0
git push origin v0.2.0
```

触发点是**标签被推送到 GitHub 的那一刻**，不是本地打标签的那一刻：`git tag` 只在本机做标记，不推送就不会有任何动作。另外两点要注意：

- 不要用一次推送多个标签的方式发版，GitHub 在**一次推送超过 3 个标签时不会产生触发事件**（`git push --tags` 很容易踩到）；逐个推即可。
- **在网页上手动创建标签同样会触发**（Tags 页的新建标签，或在「Draft a new release」里新建标签）：只要操作者是你本人，GitHub 产生就是普通的 `push` 事件。例外是标签由某个 Action 用仓库自带的 `GITHUB_TOKEN` 创建——那时 `push` 事件会被抑制（只有 `release` 事件会触发），这是自动化打标签发版常见的坑。
- 建议**只建标签，不要在网页上把 Release 一起建出来**：Release 已存在时，工作流要求 `releaseDraft` 与它当前状态一致（本工作流配的是 `false`，即已发布），否则这一步会失败；Release 说明也会以你在页面上填写的为准，不会用自动生成的。

工作流 `.github/workflows/release.yml` 在 `windows-latest` 上执行：

1. 校验标签与 `src-tauri/tauri.conf.json` 的 `version` 一致（不一致直接失败，避免发错版本号）；
2. `npm ci` + `tauri build`，把 NSIS 安装包上传到 Release；
3. 补传绿色版 `apidock.exe`；
4. 调用 GitHub Release Notes API **自动生成 Release 说明**，归类规则见 `.github/release.yml`（按 PR 标签分「新功能 / 问题修复 / 其他变更」）。

也可在 Actions 页面手动触发（Run workflow）并填入标签；标签写 `v0.2.0` 或 `0.2.0` 都可以。

### 前提条件

- Node.js + npm（前端构建）
- Rust 工具链（cargo，后端编译）
- 目标机器需有 **WebView2 Runtime**（Win10/11 一般已内置；缺失时安装包会引导下载）

### 分发说明

- 安装包**未签名**（本地打包与 CI 发布都一样），首次运行时 SmartScreen 可能提示，选择「更多信息 → 仍要运行」即可。
- 版本号在 `src-tauri/tauri.conf.json` 的 `version` 字段中修改，打包后同步到安装包文件名。
- 如需完全离线的安装包（WebView2 缺失时不联网下载），可在 `tauri.conf.json` 的 `bundle.windows.webviewInstallMode` 中改为 `embedBootstrapper` 或 `offlineInstaller` 后重新打包。