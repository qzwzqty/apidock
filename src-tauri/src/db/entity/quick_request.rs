use sea_orm::entity::prelude::*;

/// 快捷请求：与环境变量无关的临时接口（URL 为用户填写的完整地址，含 host）。
/// 归属于项目（随项目级联删除）；`name` 仅作显示名，**没有唯一键约束**（允许重名），
/// `method/url` 冗余为普通列供列表展示，`doc` 列为完整 InterfaceFile 的 JSON 文本。
/// `group_id` 为快捷请求分组（空 = 未分组，位于快捷请求根层）。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quick_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i32,
    /// 所属快捷请求分组（可空 = 未分组；分组删除时级联删除其下快捷请求）
    pub group_id: Option<i64>,
    pub name: String,
    pub method: String,
    pub url: String,
    /// 分组内手动顺序（拖拽排序写入；同值再按 id 排）
    pub sort_order: i32,
    /// 完整请求定义的 JSON 文本（InterfaceFile）
    pub doc: String,
    /// Unix 毫秒时间戳（列表按此倒序）
    pub created_at_ms: i64,
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
