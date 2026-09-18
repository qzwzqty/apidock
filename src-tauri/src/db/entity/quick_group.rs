use sea_orm::entity::prelude::*;

/// 快捷请求分组：仅用于组织快捷请求的目录层级（`parent_id` 为空 = 顶层）。
/// 归属于项目（随项目级联删除）；`name` 仅作显示名，无唯一键约束（允许重名，
/// 因此用自增 id 而不用路径键定位）。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quick_groups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i32,
    /// 父分组（空 = 顶层）；随父分组级联删除
    pub parent_id: Option<i64>,
    pub name: String,
    /// Unix 毫秒时间戳（同层按创建顺序排列）
    pub created_at_ms: i64,
    /// 同层手动顺序（拖拽排序写这一列）；0 = 未排序，回退到创建顺序
    pub sort_order: i32,
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
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
        on_delete = "Cascade"
    )]
    Parent,
}

impl Related<super::project::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Project.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
