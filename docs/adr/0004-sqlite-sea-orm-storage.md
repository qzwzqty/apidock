# ADR-0004: 改用 SQLite + sea-orm 作为唯一存储

**状态**: accepted（2026-08-21）
**Supersedes**: ADR-0002 中「以文件/文件夹为唯一存储」的结论（离线、无服务端的约束不变）

## 背景

ADR-0002 选择了「文件即真相」：团队/项目/分组为目录、接口为 JSONC 文件。随着功能演进，文件方案的管理与扩展成本不断上升：

- 重命名/移动等价于 `fs::rename`，要防御 Windows 保留名、路径长度、跨目录原子性等一系列文件系统约束；
- 多步操作（移动分组、复制接口、导入）无事务，中途失败留下半成品；
- 树构建需递归扫目录并逐文件反序列化，跨项目搜索/筛选无从实现；
- 多开冲突、并发写一致性只能靠约定（锁文件一直未实现）。

## 决策

数据根目录内改用单一 SQLite 数据库 `apidock.db` 作为唯一存储，Rust 侧通过 sea-orm（sqlx 后端）访问：

- **数据根目录语义不变**：打开一个文件夹就是全部数据，复制根目录仍可整体备份/迁移；`apidock.db` 与未来的模块数据都在根目录内。
- **表结构**：`teams / projects / groups / interfaces / environments / workspace`。分组用 `parent_id` 邻接表，每项目有一个 `parent_id IS NULL` 的根分组哨兵行，规避 SQLite 中 NULL 不参与 UNIQUE 约束的问题；唯一约束为「同父级内键唯一」，与旧目录语义一致。
- **关系表 + JSON 混合列**：实体身份字段（key/name/method 等）建普通列与索引；接口的深嵌套定义（headers/query/body 结构树/auth/assertions）整体存 `doc` JSON 列，保持整文档读写语义，避免对递归结构过度范式化。
- **一次性迁移**：首次打开含旧文件数据（`api-mgmt/`、`workspace.json`）的根目录时，在单个事务内导入数据库，成功后旧文件归档至 `.file-storage-backup-<时间戳>/`，不删除。
- **PRAGMA**：`journal_mode=WAL, synchronous=NORMAL, foreign_keys=ON, busy_timeout=5000`；桌面单用户场景使用单连接。
- **命令契约不变**：全部 Tauri 命令的入参/返回结构保持原样，前端仅移除了 `fs://changed` 监听（数据不再以外部可编辑文件存在，刷新改由命令结果驱动）。

## Considered Options

- **rusqlite / 裸 sqlx**：更轻（rusqlite 同步 API 几乎不用改函数签名），但团队选择 sea-orm 以获得迁移框架与实体类型安全，接受其 async 改造成本与编译开销。
- **全量范式化（参数/断言/body 字段全部建表）**：body 结构树是递归结构，建表收益低、维护成本高，放弃。

## Consequences

- 获得事务原子性、UNIQUE/外键约束、索引查询能力，为后续全文搜索（FTS5）、请求历史等模块铺路；
- 失去 JSONC 手工编辑/注释能力与「进 git diff/review」的工作流，数据交换改由 OpenAPI/Postman 导入导出承担；
- `notify` 文件监听移除；`storage.rs` 拆分为 `domain.rs`（纯领域模型）+ `db/`（实体/迁移/仓储）+ `legacy.rs`（旧数据导入）；
- 运行期备份提示：WAL 模式下应在应用关闭后复制根目录，或由应用提供显式备份能力。
