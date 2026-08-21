use sea_orm::entity::prelude::*;

/// 工作区状态（原 workspace.json）：单行表，固定 id = 1
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "workspace")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Vec<OpenTab> 的 JSON 文本
    pub open_tabs: String,
    pub active_tab: Option<String>,
    /// ProxyConfig 的 JSON 文本
    pub proxy: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
