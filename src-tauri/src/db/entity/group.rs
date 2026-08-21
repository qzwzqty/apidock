use sea_orm::entity::prelude::*;

/// 分组（可嵌套）。每个项目有一个根分组哨兵行：`parent_id = NULL, key = ""`，
/// 根级节点即挂在它下面，避免 SQLite 中 NULL 不参与 UNIQUE 约束的问题。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "groups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub project_id: i32,
    /// 父分组 id；根分组哨兵行为 NULL
    pub parent_id: Option<i32>,
    /// 同级唯一键（原目录名）；根分组哨兵行为空串
    pub key: String,
    pub name: String,
    pub description: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::project::Entity",
        from = "Column::ProjectId",
        to = "super::project::Column::Id"
    )]
    Project,
    #[sea_orm(has_many = "super::iface::Entity")]
    Interface,
}

impl Related<super::project::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Project.def()
    }
}

impl Related<super::iface::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Interface.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
