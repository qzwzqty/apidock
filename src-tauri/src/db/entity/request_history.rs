use sea_orm::entity::prelude::*;

/// 请求历史：每次发送的完整快照。
/// 不挂外键——删除团队/项目后历史仍可查看与重发；doc/env/global_* 为发送时的快照，
/// response/error 二选一（成功记 response，失败记 error）。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "request_history")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub team_key: String,
    pub project_key: String,
    pub project_name: String,
    pub env_id: String,
    pub env_name: String,
    pub iface_key: String,
    pub iface_name: String,
    pub method: String,
    pub url: String,
    pub status: Option<i32>,
    pub ok: bool,
    pub time_ms: i64,
    /// Unix 毫秒时间戳
    pub created_at_ms: i64,
    /// 发送时的完整接口定义（InterfaceFile JSON）
    pub doc: String,
    /// 发送时的环境快照（EnvironmentFile JSON）
    pub env_json: String,
    /// 项目全局变量（KeyValue[] JSON）
    pub global_variables: String,
    /// 项目全局参数（GlobalParams JSON）
    pub global_params: String,
    /// 响应（SendResponse JSON）
    pub response: Option<String>,
    /// 错误（SendErrorInfo JSON）
    pub error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}