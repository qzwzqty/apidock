//! 数据库迁移：M001 初始化全部表结构

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::{ColumnDef, ForeignKey, ForeignKeyAction, Index, Table};

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(M001Init),
            Box::new(M002AddProjectIndices),
            Box::new(M003AddRequestHistory),
            Box::new(M004AddHistoryRefs),
            Box::new(M005AddQuickRequests),
            Box::new(M006AddQuickGroups),
            Box::new(M007AddSortOrder),
            Box::new(M008AddGroupSortOrder),
        ]
    }
}

/// 执行全部未应用的迁移
pub async fn migrate(db: &sea_orm::DatabaseConnection) -> Result<(), String> {
    Migrator::up(db, None)
        .await
        .map_err(|e| format!("数据库迁移失败：{e}"))
}

#[derive(Iden)]
enum Teams {
    Table,
    Id,
    Key,
    Name,
    Description,
}

#[derive(Iden)]
enum Projects {
    Table,
    Id,
    TeamId,
    Key,
    Name,
    Description,
    ActiveEnvironmentId,
    GlobalVariables,
    GlobalParams,
}

#[derive(Iden)]
enum Groups {
    Table,
    Id,
    ProjectId,
    ParentId,
    Key,
    Name,
    Description,
}

#[derive(Iden)]
enum Interfaces {
    Table,
    Id,
    ProjectId,
    GroupId,
    Key,
    Name,
    Method,
    Doc,
}

#[derive(Iden)]
enum Environments {
    Table,
    Id,
    ProjectId,
    EnvId,
    FileKey,
    Name,
    Host,
    Builtin,
    Variables,
}

/// 版本名固定为历史遗留值 "migration"（旧版 DeriveMigrationName 的产物）。
/// 已建库的 seaql_migrations 里记录的就是它，改其他名会被误判为"缺失迁移"而拒绝启动；
/// 新库同样以此名记录。后续新增迁移一律手动实现 MigrationName 并使用唯一名。
pub struct M001Init;

impl MigrationName for M001Init {
    fn name(&self) -> &str {
        "migration"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M001Init {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // teams
        manager
            .create_table(
                Table::create()
                    .table(Teams::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Teams::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Teams::Key).string_len(255).not_null().unique_key())
                    .col(ColumnDef::new(Teams::Name).string_len(255).not_null())
                    .col(ColumnDef::new(Teams::Description).text().not_null().default(""))
                    .to_owned(),
            )
            .await?;

        // projects
        manager
            .create_table(
                Table::create()
                    .table(Projects::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Projects::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Projects::TeamId).integer().not_null())
                    .col(ColumnDef::new(Projects::Key).string_len(255).not_null())
                    .col(ColumnDef::new(Projects::Name).string_len(255).not_null())
                    .col(ColumnDef::new(Projects::Description).text().not_null().default(""))
                    .col(ColumnDef::new(Projects::ActiveEnvironmentId).string_len(255))
                    .col(
                        ColumnDef::new(Projects::GlobalVariables)
                            .text()
                            .not_null()
                            .default("[]"),
                    )
                    .col(
                        ColumnDef::new(Projects::GlobalParams)
                            .text()
                            .not_null()
                            .default("{}"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_projects_team")
                            .from(Projects::Table, Projects::TeamId)
                            .to(Teams::Table, Teams::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // groups（parent_id 可空：项目根分组哨兵行为 NULL）
        manager
            .create_table(
                Table::create()
                    .table(Groups::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Groups::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Groups::ProjectId).integer().not_null())
                    .col(ColumnDef::new(Groups::ParentId).integer())
                    .col(ColumnDef::new(Groups::Key).string_len(255).not_null())
                    .col(ColumnDef::new(Groups::Name).string_len(255).not_null())
                    .col(ColumnDef::new(Groups::Description).text().not_null().default(""))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_groups_project")
                            .from(Groups::Table, Groups::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_groups_parent")
                            .from(Groups::Table, Groups::ParentId)
                            .to(Groups::Table, Groups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // interfaces
        manager
            .create_table(
                Table::create()
                    .table(Interfaces::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Interfaces::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Interfaces::ProjectId).integer().not_null())
                    .col(ColumnDef::new(Interfaces::GroupId).integer().not_null())
                    .col(ColumnDef::new(Interfaces::Key).string_len(255).not_null())
                    .col(ColumnDef::new(Interfaces::Name).string_len(255).not_null())
                    .col(
                        ColumnDef::new(Interfaces::Method)
                            .string_len(16)
                            .not_null()
                            .default("GET"),
                    )
                    .col(ColumnDef::new(Interfaces::Doc).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_interfaces_project")
                            .from(Interfaces::Table, Interfaces::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_interfaces_group")
                            .from(Interfaces::Table, Interfaces::GroupId)
                            .to(Groups::Table, Groups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // environments
        manager
            .create_table(
                Table::create()
                    .table(Environments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Environments::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Environments::ProjectId).integer().not_null())
                    .col(ColumnDef::new(Environments::EnvId).string_len(255).not_null())
                    .col(ColumnDef::new(Environments::FileKey).string_len(255).not_null())
                    .col(ColumnDef::new(Environments::Name).string_len(255).not_null())
                    .col(ColumnDef::new(Environments::Host).text().not_null().default(""))
                    .col(
                        ColumnDef::new(Environments::Builtin)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Environments::Variables)
                            .text()
                            .not_null()
                            .default("[]"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_environments_project")
                            .from(Environments::Table, Environments::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // workspace（单行表，id 恒为 1）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS workspace (
                    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
                    open_tabs TEXT NOT NULL DEFAULT '[]',
                    active_tab TEXT,
                    proxy TEXT NOT NULL DEFAULT '{}'
                )",
            )
            .await?;

        // ---- 唯一索引 ----
        let idx = |name: &str| name.to_string();

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(idx("uq_projects_team_key"))
                    .table(Projects::Table)
                    .col(Projects::TeamId)
                    .col(Projects::Key)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // parent_id 非空的行参与唯一约束；根分组由下方部分索引保证每项目一个
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(idx("uq_groups_parent_key"))
                    .table(Groups::Table)
                    .col(Groups::ParentId)
                    .col(Groups::Key)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(idx("uq_interfaces_group_key"))
                    .table(Interfaces::Table)
                    .col(Interfaces::GroupId)
                    .col(Interfaces::Key)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(idx("uq_environments_project_env_id"))
                    .table(Environments::Table)
                    .col(Environments::ProjectId)
                    .col(Environments::EnvId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(idx("uq_environments_project_file_key"))
                    .table(Environments::Table)
                    .col(Environments::ProjectId)
                    .col(Environments::FileKey)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 每个项目至多一个根分组哨兵行（SQLite 的 UNIQUE 对 NULL 互不冲突，需部分索引兜底）
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX IF NOT EXISTS uq_groups_one_root
                 ON groups (project_id) WHERE parent_id IS NULL",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for t in ["interfaces", "environments", "groups", "projects", "teams", "workspace"] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {t}"))
                .await?;
        }
        Ok(())
    }
}

/// 按 project_id 过滤的查询（整树加载/级联删除/后续搜索）补普通索引
pub struct M002AddProjectIndices;

impl MigrationName for M002AddProjectIndices {
    fn name(&self) -> &str {
        "m002_add_project_indices"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M002AddProjectIndices {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_groups_project_id")
                    .table(Groups::Table)
                    .col(Groups::ProjectId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_interfaces_project_id")
                    .table(Interfaces::Table)
                    .col(Interfaces::ProjectId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_groups_project_id")
                    .table(Groups::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_interfaces_project_id")
                    .table(Interfaces::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum RequestHistory {
    Table,
    Id,
    TeamKey,
    ProjectKey,
    ProjectName,
    EnvId,
    EnvName,
    IfaceKey,
    IfaceName,
    Method,
    Url,
    Status,
    Ok,
    TimeMs,
    CreatedAtMs,
    Doc,
    EnvJson,
    GlobalVariables,
    GlobalParams,
    Response,
    Error,
    TeamId,
    ProjectId,
    GroupId,
    IfaceId,
}

/// 请求历史表：无外键（快照自包含），按时间倒序检索。
pub struct M003AddRequestHistory;

impl MigrationName for M003AddRequestHistory {
    fn name(&self) -> &str {
        "m003_add_request_history"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M003AddRequestHistory {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RequestHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RequestHistory::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RequestHistory::TeamKey).string_len(255).not_null())
                    .col(ColumnDef::new(RequestHistory::ProjectKey).string_len(255).not_null())
                    .col(ColumnDef::new(RequestHistory::ProjectName).string_len(255).not_null().default(""))
                    .col(ColumnDef::new(RequestHistory::EnvId).string_len(255).not_null().default(""))
                    .col(ColumnDef::new(RequestHistory::EnvName).string_len(255).not_null().default(""))
                    .col(ColumnDef::new(RequestHistory::IfaceKey).string_len(255).not_null().default(""))
                    .col(ColumnDef::new(RequestHistory::IfaceName).string_len(255).not_null().default(""))
                    .col(ColumnDef::new(RequestHistory::Method).string_len(16).not_null().default("GET"))
                    .col(ColumnDef::new(RequestHistory::Url).text().not_null().default(""))
                    .col(ColumnDef::new(RequestHistory::Status).integer())
                    .col(ColumnDef::new(RequestHistory::Ok).boolean().not_null().default(false))
                    .col(ColumnDef::new(RequestHistory::TimeMs).big_integer().not_null().default(0))
                    .col(ColumnDef::new(RequestHistory::CreatedAtMs).big_integer().not_null())
                    .col(ColumnDef::new(RequestHistory::Doc).text().not_null().default("{}"))
                    .col(ColumnDef::new(RequestHistory::EnvJson).text().not_null().default("{}"))
                    .col(ColumnDef::new(RequestHistory::GlobalVariables).text().not_null().default("[]"))
                    .col(ColumnDef::new(RequestHistory::GlobalParams).text().not_null().default("{}"))
                    .col(ColumnDef::new(RequestHistory::Response).text())
                    .col(ColumnDef::new(RequestHistory::Error).text())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_request_history_created_at")
                    .table(RequestHistory::Table)
                    .col(RequestHistory::CreatedAtMs)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS request_history")
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum QuickRequests {
    Table,
    Id,
    ProjectId,
    Name,
    Method,
    Url,
    Doc,
    CreatedAtMs,
}

/// 快捷请求表：不依赖环境的临时接口（用户填写完整地址），按项目归属、随项目级联删除。
/// 刻意不建唯一索引：名称允许重复（无唯一键要求）。
pub struct M005AddQuickRequests;

impl MigrationName for M005AddQuickRequests {
    fn name(&self) -> &str {
        "m005_add_quick_requests"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M005AddQuickRequests {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(QuickRequests::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(QuickRequests::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(QuickRequests::ProjectId).integer().not_null())
                    .col(ColumnDef::new(QuickRequests::Name).string_len(255).not_null())
                    .col(
                        ColumnDef::new(QuickRequests::Method)
                            .string_len(16)
                            .not_null()
                            .default("GET"),
                    )
                    .col(ColumnDef::new(QuickRequests::Url).text().not_null().default(""))
                    .col(ColumnDef::new(QuickRequests::Doc).text().not_null().default("{}"))
                    .col(
                        ColumnDef::new(QuickRequests::CreatedAtMs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_quick_requests_project")
                            .from(QuickRequests::Table, QuickRequests::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_quick_requests_project_id")
                    .table(QuickRequests::Table)
                    .col(QuickRequests::ProjectId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS quick_requests")
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum QuickGroups {
    Table,
    Id,
    ProjectId,
    ParentId,
    Name,
    CreatedAtMs,
}

/// 快捷请求分组：组织快捷请求的目录层级（parent_id 为空 = 顶层，可嵌套）。
/// 与接口分组不同，名称无唯一键、不需路径键，故子表用自增 id 定位。
pub struct M006AddQuickGroups;

impl MigrationName for M006AddQuickGroups {
    fn name(&self) -> &str {
        "m006_add_quick_groups"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M006AddQuickGroups {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(QuickGroups::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(QuickGroups::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(QuickGroups::ProjectId).integer().not_null())
                    .col(ColumnDef::new(QuickGroups::ParentId).big_integer().null())
                    .col(ColumnDef::new(QuickGroups::Name).string_len(255).not_null())
                    .col(
                        ColumnDef::new(QuickGroups::CreatedAtMs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_quick_groups_project")
                            .from(QuickGroups::Table, QuickGroups::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_quick_groups_parent")
                            .from(QuickGroups::Table, QuickGroups::ParentId)
                            .to(QuickGroups::Table, QuickGroups::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_quick_groups_project_id")
                    .table(QuickGroups::Table)
                    .col(QuickGroups::ProjectId)
                    .to_owned(),
            )
            .await?;

        // 快捷请求挂到分组上（可空 = 未分组）。SQLite 要求新增的 REFERENCES 列默认值为 NULL，
        // 故用原生语句补建外键，删除分组时级联删除其下快捷请求。
        if !manager
            .has_column("quick_requests", "group_id")
            .await
            .unwrap_or(false)
        {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE quick_requests ADD COLUMN group_id INTEGER \
                     REFERENCES quick_groups(id) ON DELETE CASCADE",
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager
            .alter_table(
                Table::alter()
                    .table(QuickRequests::Table)
                    .drop_column("group_id")
                    .to_owned(),
            )
            .await
        {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("no such column") && !msg.contains("duplicate column") {
                    return Err(e);
                }
            }
        }
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS quick_groups")
            .await?;
        Ok(())
    }
}

/// 接口与快捷请求的手动排序列（拖拽排序即写这一列）：
/// - 同层内分组恒在前面（按 key 排），其后是接口，按 `sort_order` 升序、同值再按 key
/// - 旧数据默认为 0（等价于原先的字母序），新建/拖拽后会重排为 1..n
pub struct M007AddSortOrder;

impl MigrationName for M007AddSortOrder {
    fn name(&self) -> &str {
        "m007_add_sort_order"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M007AddSortOrder {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["interfaces", "quick_requests"] {
            if !manager
                .has_column(table, "sort_order")
                .await
                .unwrap_or(false)
            {
                manager
                    .get_connection()
                    .execute_unprepared(&format!(
                        "ALTER TABLE {table} ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0"
                    ))
                    .await?;
            }
        }
        // 快捷请求原先按创建时间倒序展示，引入手动顺序后按 id 升序回填（保持稳定、可预期的顺序）
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE quick_requests SET sort_order = id WHERE sort_order = 0 AND id > 0",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["interfaces", "quick_requests"] {
            match manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(table))
                        .drop_column(Alias::new("sort_order"))
                        .to_owned(),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if !msg.contains("no such column") && !msg.contains("duplicate column") {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }
}

/// 分组的手动排序列（拖拽排序/移动分组写这一列），接口分组与快捷请求分组各一份：
/// - 同一层内分组恒定排在接口之前，`sort_order` 决定同层分组之间的先后
/// - 旧数据默认为 0（同值按原规则退化为 key / 创建时间排序），新建或拖拽后重排为 1..n
pub struct M008AddGroupSortOrder;

impl MigrationName for M008AddGroupSortOrder {
    fn name(&self) -> &str {
        "m008_add_group_sort_order"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M008AddGroupSortOrder {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["groups", "quick_groups"] {
            if !manager
                .has_column(table, "sort_order")
                .await
                .unwrap_or(false)
            {
                manager
                    .get_connection()
                    .execute_unprepared(&format!(
                        "ALTER TABLE {table} ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0"
                    ))
                    .await?;
            }
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["groups", "quick_groups"] {
            match manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(table))
                        .drop_column(Alias::new("sort_order"))
                        .to_owned(),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    let msg = e.to_string();
                    if !msg.contains("no such column") && !msg.contains("duplicate column") {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }
}

/// 请求历史补充引用 id 列（团队/项目/分组/接口的数据库主键，可空）。
/// 只是"外键 id"值，不建约束：删除团队/项目/接口后历史仍可查看与重发（快照自包含），
/// 引用 id 仅用于前端跳转/关联展示。
pub struct M004AddHistoryRefs;

impl MigrationName for M004AddHistoryRefs {
    fn name(&self) -> &str {
        "m004_add_history_refs"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M004AddHistoryRefs {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (col, iden) in [
            (RequestHistory::TeamId, "team_id"),
            (RequestHistory::ProjectId, "project_id"),
            (RequestHistory::GroupId, "group_id"),
            (RequestHistory::IfaceId, "iface_id"),
        ] {
            if !manager
                .has_column("request_history", iden)
                .await
                .unwrap_or(false)
            {
                manager
                    .alter_table(
                        Table::alter()
                            .table(RequestHistory::Table)
                            .add_column(ColumnDef::new(col).integer().null())
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (col, _iden) in [
            (RequestHistory::TeamId, "team_id"),
            (RequestHistory::ProjectId, "project_id"),
            (RequestHistory::GroupId, "group_id"),
            (RequestHistory::IfaceId, "iface_id"),
        ] {
            match manager
                .alter_table(
                    Table::alter()
                        .table(RequestHistory::Table)
                        .drop_column(col)
                        .to_owned(),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    // 列不存在时忽略；其他错误如实返回
                    let msg = e.to_string();
                    if !msg.contains("no such column") && !msg.contains("duplicate column") {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }
}
