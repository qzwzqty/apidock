use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "projects")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub team_id: i32,
    /// 唯一键（原目录名），团队内唯一
    pub key: String,
    pub name: String,
    pub description: String,
    /// 当前激活环境 id（如 env-prod）
    pub active_environment_id: Option<String>,
    /// 全局变量 Vec<KeyValue> 的 JSON 文本
    pub global_variables: String,
    /// 全局参数 GlobalParams 的 JSON 文本
    pub global_params: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
    #[sea_orm(has_many = "super::group::Entity")]
    Group,
    #[sea_orm(has_many = "super::iface::Entity")]
    Interface,
    #[sea_orm(has_many = "super::environment::Entity")]
    Environment,
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl Related<super::group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl Related<super::iface::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Interface.def()
    }
}

impl Related<super::environment::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Environment.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
