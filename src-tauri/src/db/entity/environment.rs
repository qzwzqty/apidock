use sea_orm::entity::prelude::*;

/// 环境（原 environments/*.json）
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "environments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub project_id: i32,
    /// 业务 id（如 env-prod），项目内唯一
    pub env_id: String,
    /// 原文件名键（如 prod），项目内唯一
    pub file_key: String,
    pub name: String,
    pub host: String,
    pub builtin: bool,
    /// Vec<KeyValue> 的 JSON 文本
    pub variables: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::project::Entity",
        from = "Column::ProjectId",
        to = "super::project::Column::Id"
    )]
    Project,
}

impl Related<super::project::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Project.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
