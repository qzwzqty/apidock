use sea_orm::entity::prelude::*;

/// 接口（原 JSONC 文件）。`doc` 列为完整 InterfaceFile 的 JSON 文本，
/// `key/name/method` 冗余为普通列供树构建与检索。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "interfaces")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub project_id: i32,
    /// 所属分组（含项目根分组哨兵）
    pub group_id: i32,
    /// 分组内唯一键（原文件名，不含 .json）
    pub key: String,
    pub name: String,
    pub method: String,
    /// 完整接口定义的 JSON 文本（InterfaceFile）
    pub doc: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::project::Entity",
        from = "Column::ProjectId",
        to = "super::project::Column::Id"
    )]
    Project,
    #[sea_orm(
        belongs_to = "super::group::Entity",
        from = "Column::GroupId",
        to = "super::group::Column::Id"
    )]
    Group,
}

impl Related<super::project::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Project.def()
    }
}

impl Related<super::group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
