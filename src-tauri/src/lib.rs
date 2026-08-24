mod assertions;
mod db;
mod domain;
mod http;
mod imports;
mod runner;
mod variables;

use db::repo;
use domain::{
    EnvironmentFile, HistoryRecord, HistorySummary, InterfaceFile, ProjectInfo, ProjectSettings,
    TeamInfo, TreeNode, WorkspaceState,
};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

pub struct AppState {
    db: Mutex<Option<DatabaseConnection>>,
    cookiejar: Arc<reqwest::cookie::Jar>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            db: Mutex::new(None),
            cookiejar: Arc::new(reqwest::cookie::Jar::default()),
        }
    }
}

/// 一次会话的初始快照：团队列表 + 标签栏状态
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSession {
    pub teams: Vec<TeamInfo>,
    pub workspace: WorkspaceState,
}

// ----- 会话 / 数据根 -----

#[tauri::command]
async fn get_session(state: State<'_, AppState>) -> Result<AppSession, String> {
    ensure_open(&state).await?;
    build_session(&state).await
}

/// 若数据库未打开则打开固定的默认数据根 <home>/.apidock（自动创建）
async fn ensure_open(state: &AppState) -> Result<(), String> {
    let already = state.db.lock().unwrap().is_some();
    if already {
        return Ok(());
    }
    let root = default_data_root().ok_or("无法确定用户主目录")?;
    let db = db::open(&root).await?;
    *state.db.lock().unwrap() = Some(db);
    Ok(())
}

/// 默认数据根目录：`<用户主目录>/.apidock`（主目录路径随操作系统而定）
fn default_data_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".apidock"))
}

fn with_db(state: &AppState) -> Result<DatabaseConnection, String> {
    state
        .db
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "数据尚未就绪".to_string())
}

async fn build_session(state: &AppState) -> Result<AppSession, String> {
    let db = state.db.lock().unwrap().clone();
    match db {
        Some(db) => Ok(AppSession {
            teams: repo::list_teams(&db).await,
            workspace: repo::get_workspace(&db).await,
        }),
        None => Ok(AppSession {
            teams: Vec::new(),
            workspace: WorkspaceState::new(),
        }),
    }
}

// ----- 团队 / 项目 -----

#[tauri::command]
async fn list_teams(state: State<'_, AppState>) -> Result<Vec<TeamInfo>, String> {
    let db = with_db(&state)?;
    Ok(repo::list_teams(&db).await)
}

#[tauri::command]
async fn list_projects(
    state: State<'_, AppState>,
    team_key: String,
) -> Result<Vec<ProjectInfo>, String> {
    let db = with_db(&state)?;
    Ok(repo::list_projects(&db, &team_key).await)
}

#[tauri::command]
async fn create_team(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<TeamInfo, String> {
    let db = with_db(&state)?;
    let name = domain::validate_name(&name)?;
    if repo::list_teams(&db).await.iter().any(|t| t.key == name) {
        return Err("已存在同名团队".into());
    }
    let team = repo::create_team(&db, &name, &name).await?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            repo::set_team_description(&db, &name, desc.trim()).await?;
        }
    }
    Ok(team)
}

#[tauri::command]
async fn create_project(
    state: State<'_, AppState>,
    team_key: String,
    name: String,
    description: Option<String>,
) -> Result<ProjectInfo, String> {
    let db = with_db(&state)?;
    let name = domain::validate_name(&name)?;
    if repo::list_projects(&db, &team_key)
        .await
        .iter()
        .any(|p| p.key == name)
    {
        return Err("已存在同名项目".into());
    }
    let project = repo::create_project(&db, &team_key, &name, &name).await?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            repo::set_project_description(&db, &team_key, &name, desc.trim()).await?;
        }
    }
    Ok(project)
}

#[tauri::command]
async fn delete_team(state: State<'_, AppState>, team_key: String) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::delete_team(&db, &team_key).await
}

#[tauri::command]
async fn delete_project(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::delete_project(&db, &team_key, &project_key).await
}

#[tauri::command]
async fn rename_team(
    state: State<'_, AppState>,
    team_key: String,
    new_name: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    let new_name = domain::validate_name(&new_name)?;
    if new_name != team_key && repo::list_teams(&db).await.iter().any(|t| t.key == new_name) {
        return Err("已存在同名团队".into());
    }
    repo::rename_team(&db, &team_key, &new_name).await
}

#[tauri::command]
async fn rename_project(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    new_name: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    let new_name = domain::validate_name(&new_name)?;
    if new_name != project_key
        && repo::list_projects(&db, &team_key)
            .await
            .iter()
            .any(|p| p.key == new_name)
    {
        return Err("已存在同名项目".into());
    }
    repo::rename_project(&db, &team_key, &project_key, &new_name).await
}

#[tauri::command]
async fn move_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    target_group_path: Vec<String>,
) -> Result<String, String> {
    let db = with_db(&state)?;
    repo::move_interface(
        &db,
        &team_key,
        &project_key,
        &group_path,
        &iface_key,
        &target_group_path,
    )
    .await
}

#[tauri::command]
async fn move_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    target_group_path: Vec<String>,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::move_group(&db, &team_key, &project_key, &group_path, &target_group_path).await
}

#[tauri::command]
async fn save_workspace(
    state: State<'_, AppState>,
    workspace: WorkspaceState,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::save_workspace(&db, &workspace).await
}

// ----- 接口 / 分组树 -----

#[tauri::command]
async fn list_interface_tree(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<Vec<TreeNode>, String> {
    let db = with_db(&state)?;
    Ok(repo::list_interface_tree(&db, &team_key, &project_key).await)
}

/// 取接口树下某分组路径下的直接子节点键（含分组键与接口键）
fn dir_keys(nodes: &[TreeNode], at: &[String]) -> Vec<String> {
    if at.is_empty() {
        return nodes
            .iter()
            .map(|n| match n {
                TreeNode::Group { key, .. } | TreeNode::Interface { key, .. } => key.clone(),
            })
            .collect();
    }
    for n in nodes {
        if let TreeNode::Group { key, children, .. } = n {
            if key == &at[0] {
                return dir_keys(children, &at[1..]);
            }
        }
    }
    Vec::new()
}

#[tauri::command]
async fn create_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    let db = with_db(&state)?;
    let name = domain::validate_name(&name)?;
    let keys = dir_keys(
        &repo::list_interface_tree(&db, &team_key, &project_key).await,
        &group_path,
    );
    if keys.contains(&name) {
        return Err("已存在同名分组/接口".into());
    }
    repo::create_group(&db, &team_key, &project_key, &group_path, &name, &name).await?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            repo::set_group_description(&db, &team_key, &project_key, &group_path, desc.trim())
                .await?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn rename_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    new_name: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    let new_name = domain::validate_name(&new_name)?;
    let keys = dir_keys(
        &repo::list_interface_tree(&db, &team_key, &project_key).await,
        &group_path[..group_path.len().saturating_sub(1)],
    );
    if keys.iter().any(|k| {
        k == &new_name && k != group_path.last().map(String::as_str).unwrap_or_default()
    }) {
        return Err("已存在同名分组/接口".into());
    }
    repo::rename_group(&db, &team_key, &project_key, &group_path, &new_name).await
}

#[tauri::command]
async fn delete_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::delete_group(&db, &team_key, &project_key, &group_path).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedInterface {
    key: String,
    file: InterfaceFile,
}

#[tauri::command]
async fn create_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    name: String,
    description: Option<String>,
) -> Result<CreatedInterface, String> {
    let db = with_db(&state)?;
    let name = domain::validate_name(&name)?;
    let keys = dir_keys(
        &repo::list_interface_tree(&db, &team_key, &project_key).await,
        &group_path,
    );
    if keys.contains(&name) {
        return Err("已存在同名分组/接口".into());
    }
    let mut iface =
        repo::create_interface(&db, &team_key, &project_key, &group_path, &name, &name).await?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            iface.description = desc.trim().to_string();
            repo::save_interface(&db, &team_key, &project_key, &group_path, &name, &iface).await?;
        }
    }
    Ok(CreatedInterface { key: name, file: iface })
}

/// 复制接口到同分组：内容完全一致，名称 = 原名后接 -copy（已存在则 -copy-2 / -copy-3 …），新 id
#[tauri::command]
async fn copy_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
) -> Result<CreatedInterface, String> {
    let db = with_db(&state)?;
    let src = repo::get_interface(&db, &team_key, &project_key, &group_path, &iface_key).await?;
    let keys = dir_keys(
        &repo::list_interface_tree(&db, &team_key, &project_key).await,
        &group_path,
    );
    let key = unique_copy_key(&iface_key, &keys);
    let created =
        repo::create_interface(&db, &team_key, &project_key, &group_path, &key, &key).await?;
    let mut doc = src;
    doc.id = created.id;
    doc.name = created.name;
    repo::save_interface(&db, &team_key, &project_key, &group_path, &key, &doc).await?;
    Ok(CreatedInterface { key, file: doc })
}

/// 复制后的键名：原名后接 -copy；本身已是副本（-copy 或 -copy-N）时以其原始名为基础
fn unique_copy_key(iface_key: &str, keys: &[String]) -> String {
    let stem = if let Some(k) = iface_key.strip_suffix("-copy") {
        k.to_string()
    } else if let Some(idx) = iface_key.rfind("-copy-") {
        if iface_key[idx + 6..].chars().all(|c| c.is_ascii_digit()) {
            iface_key[..idx].to_string()
        } else {
            iface_key.to_string()
        }
    } else {
        iface_key.to_string()
    };
    let mut key = format!("{stem}-copy");
    let mut n = 1;
    while keys.iter().any(|k| k == &key) {
        n += 1;
        key = format!("{stem}-copy-{n}");
    }
    key
}

#[tauri::command]
async fn get_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
) -> Result<InterfaceFile, String> {
    let db = with_db(&state)?;
    repo::get_interface(&db, &team_key, &project_key, &group_path, &iface_key).await
}

#[tauri::command]
async fn save_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    iface: InterfaceFile,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::save_interface(&db, &team_key, &project_key, &group_path, &iface_key, &iface).await
}

#[tauri::command]
async fn rename_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    new_name: String,
) -> Result<String, String> {
    let db = with_db(&state)?;
    let new_name = domain::validate_name(&new_name)?;
    if new_name != iface_key {
        let keys = dir_keys(
            &repo::list_interface_tree(&db, &team_key, &project_key).await,
            &group_path,
        );
        if keys.iter().any(|k| k == &new_name) {
            return Err("已存在同名分组/接口".into());
        }
    }
    repo::rename_interface(&db, &team_key, &project_key, &group_path, &iface_key, &new_name)
        .await?;
    Ok(new_name)
}

#[tauri::command]
async fn delete_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::delete_interface(&db, &team_key, &project_key, &group_path, &iface_key).await
}

// ----- 环境 / 项目设置 -----

#[tauri::command]
async fn list_environments(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<Vec<domain::EnvironmentSummary>, String> {
    let db = with_db(&state)?;
    Ok(repo::list_environments(&db, &team_key, &project_key).await)
}

#[tauri::command]
async fn get_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
) -> Result<EnvironmentFile, String> {
    let db = with_db(&state)?;
    repo::get_environment(&db, &team_key, &project_key, &env_id).await
}

#[tauri::command]
async fn save_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env: EnvironmentFile,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::save_environment(&db, &team_key, &project_key, env).await
}

#[tauri::command]
async fn delete_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::delete_environment(&db, &team_key, &project_key, &env_id).await
}

#[tauri::command]
async fn set_active_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::set_active_environment(&db, &team_key, &project_key, &env_id).await
}

#[tauri::command]
async fn get_project_settings(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<ProjectSettings, String> {
    let db = with_db(&state)?;
    repo::get_project_settings(&db, &team_key, &project_key).await
}

#[tauri::command]
async fn save_project_settings(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    settings: ProjectSettings,
) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::save_project_settings(&db, &team_key, &project_key, settings).await
}

// ----- 发送请求 -----

/// 把任意可序列化值转 JSON 文本（序列化失败返回 None；此处用于历史快照，尽力而为）
fn to_json_str<T: serde::Serialize>(v: &T) -> Option<String> {
    serde_json::to_string(v).ok()
}

/// 把一次发送（成功或失败）写入请求历史。失败是尽力而为（不阻断发送结果）。
/// 返回新记录（供 resend_history 直接回传前端）。
async fn record_history(
    db: &sea_orm::DatabaseConnection,
    team_key: &str,
    project_key: &str,
    settings: &ProjectSettings,
    env: &EnvironmentFile,
    iface: &InterfaceFile,
    iface_key: Option<String>,
    iface_name: Option<String>,
    result: &Result<http::SendResponse, http::SendErrorInfo>,
) -> Option<HistoryRecord> {
    let input = match result {
        Ok(res) => repo::HistoryInput {
            team_key: team_key.into(),
            project_key: project_key.into(),
            project_name: settings.name.clone(),
            env_id: env.id.clone(),
            env_name: env.name.clone(),
            iface_key: iface_key.unwrap_or_default(),
            iface_name: iface_name.unwrap_or_else(|| iface.name.clone()),
            method: iface.method.clone(),
            url: if res.resolved_url.trim().is_empty() {
                iface.url.clone()
            } else {
                res.resolved_url.clone()
            },
            status: Some(res.status),
            ok: true,
            time_ms: res.time_ms,
            created_at_ms: domain::now_unix_ms(),
            doc_json: to_json_str(iface).unwrap_or_default(),
            env_json: to_json_str(env).unwrap_or_default(),
            global_variables_json: to_json_str(&settings.global_variables).unwrap_or_default(),
            global_params_json: to_json_str(&settings.global_params).unwrap_or_default(),
            response_json: to_json_str(res),
            error_json: None,
        },
        Err(err) => repo::HistoryInput {
            team_key: team_key.into(),
            project_key: project_key.into(),
            project_name: settings.name.clone(),
            env_id: env.id.clone(),
            env_name: env.name.clone(),
            iface_key: iface_key.unwrap_or_default(),
            iface_name: iface_name.unwrap_or_else(|| iface.name.clone()),
            method: iface.method.clone(),
            url: iface.url.clone(),
            status: None,
            ok: false,
            time_ms: 0,
            created_at_ms: domain::now_unix_ms(),
            doc_json: to_json_str(iface).unwrap_or_default(),
            env_json: to_json_str(env).unwrap_or_default(),
            global_variables_json: to_json_str(&settings.global_variables).unwrap_or_default(),
            global_params_json: to_json_str(&settings.global_params).unwrap_or_default(),
            response_json: None,
            error_json: to_json_str(err),
        },
    };
    match repo::insert_history(db, input).await {
        Ok(model) => Some(repo::history_record(&model)),
        Err(e) => {
            eprintln!("记录请求历史失败：{e}");
            None
        }
    }
}

#[tauri::command]
async fn send_request(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
    iface: InterfaceFile,
    iface_key: Option<String>,
    iface_name: Option<String>,
) -> Result<http::SendResponse, http::SendErrorInfo> {
    let db = state
        .db
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| http::SendErrorInfo {
            kind: "http".into(),
            message: "尚未选择数据根目录".into(),
        })?;
    let http_err = |message: String| http::SendErrorInfo { kind: "http".into(), message };
    let env = repo::get_environment(&db, &team_key, &project_key, &env_id)
        .await
        .map_err(http_err)?;
    let settings = repo::get_project_settings(&db, &team_key, &project_key)
        .await
        .map_err(http_err)?;
    let opts = http::SendOptions {
        proxy: repo::get_workspace(&db).await.proxy,
        cookie_jar: Some(state.cookiejar.clone()),
        ..Default::default()
    };
    let result = http::send(
        &iface,
        &env,
        &settings.global_variables,
        &settings.global_params,
        &opts,
    )
    .await;
    record_history(
        &db,
        &team_key,
        &project_key,
        &settings,
        &env,
        &iface,
        iface_key,
        iface_name,
        &result,
    )
    .await;
    result
}

// ----- 请求历史 -----

#[tauri::command]
async fn list_request_history(
    state: State<'_, AppState>,
) -> Result<Vec<HistorySummary>, String> {
    let db = with_db(&state)?;
    Ok(repo::list_history(&db).await)
}

#[tauri::command]
async fn get_request_history(state: State<'_, AppState>, id: i64) -> Result<HistoryRecord, String> {
    let db = with_db(&state)?;
    repo::get_history(&db, id).await
}

#[tauri::command]
async fn delete_request_history(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::delete_history(&db, id).await
}

#[tauri::command]
async fn clear_request_history(state: State<'_, AppState>) -> Result<(), String> {
    let db = with_db(&state)?;
    repo::clear_history(&db).await
}

/// 按历史快照重发请求：使用记录时的环境/全局变量快照（项目/环境被删后仍可重发），
/// 并使用当前代理与会话；重发本身记为新的一条历史。
/// `iface` 可传入前端编辑后的接口定义（重发用编辑后的文档，环境/全局变量仍用快照）。
#[tauri::command]
async fn resend_history(
    state: State<'_, AppState>,
    id: i64,
    iface: Option<InterfaceFile>,
) -> Result<HistoryRecord, String> {
    let db = with_db(&state)?;
    let rec = repo::get_history(&db, id).await?;
    let doc = iface.unwrap_or_else(|| rec.doc.clone());
    let opts = http::SendOptions {
        proxy: repo::get_workspace(&db).await.proxy,
        cookie_jar: Some(state.cookiejar.clone()),
        ..Default::default()
    };
    let result = http::send(
        &doc,
        &rec.env,
        &rec.global_variables,
        &rec.global_params,
        &opts,
    )
    .await;
    let settings = ProjectSettings {
        name: rec.project_name.clone(),
        active_environment_id: Some(rec.env_id.clone()),
        global_variables: rec.global_variables.clone(),
        global_params: rec.global_params.clone(),
    };
    record_history(
        &db,
        &rec.team_key,
        &rec.project_key,
        &settings,
        &rec.env,
        &doc,
        Some(rec.iface_key.clone()),
        Some(rec.iface_name.clone()),
        &result,
    )
    .await
    .ok_or_else(|| "重发失败：未能写入历史".to_string())
}

#[tauri::command]
async fn run_interfaces(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
) -> Result<runner::RunReport, String> {
    let db = with_db(&state)?;
    runner::run_project(
        &db,
        &team_key,
        &project_key,
        Some(&group_path).filter(|g| !g.is_empty()).map(|x| x.as_slice()),
    )
    .await
}

// ----- 导入 / 导出 -----

fn detect_spec_kind(content: &str) -> (&'static str, bool) {
    let trimmed = content.trim_start();
    if trimmed.starts_with('{') {
        if trimmed.contains("\"openapi\"") && trimmed.contains("\"paths\"") {
            return ("openapi", false);
        }
        if trimmed.contains("\"item\"") && trimmed.contains("\"info\"") {
            return ("postman", false);
        }
        ("openapi", false)
    } else {
        // 尝试 YAML
        ("openapi", true)
    }
}

#[tauri::command]
async fn import_spec_into_project(
    state: State<'_, AppState>,
    path: String,
    team_key: String,
    project_key: String,
) -> Result<(imports::ImportReport, String), String> {
    let db = with_db(&state)?;
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{e}"))?;
    let (kind, is_yaml) = detect_spec_kind(&content);
    let (name, ifaces) = match kind {
        "postman" => imports::parse_postman(&content)?,
        _ => imports::parse_openapi(&content, is_yaml)?,
    };
    let report = imports::import_into_project(&db, &team_key, &project_key, &ifaces).await?;
    Ok((report, name))
}

#[tauri::command]
async fn import_spec_new_project(
    state: State<'_, AppState>,
    path: String,
    team_key: String,
) -> Result<(imports::ImportReport, String), String> {
    let db = with_db(&state)?;
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{e}"))?;
    let (kind, is_yaml) = detect_spec_kind(&content);
    let (name, ifaces) = match kind {
        "postman" => imports::parse_postman(&content)?,
        _ => imports::parse_openapi(&content, is_yaml)?,
    };
    let project_key = match domain::validate_name(&name) {
        Ok(k) => k,
        Err(_) => {
            let replaced: String = name
                .trim()
                .chars()
                .map(|c| {
                    if c <= '\u{1f}'
                        || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                    {
                        '-'
                    } else {
                        c
                    }
                })
                .collect();
            let replaced = replaced
                .trim_end_matches(|c| c == '.' || c == '-' || c == ' ')
                .to_string();
            if replaced.is_empty() {
                format!("imported-{}", uuid::Uuid::new_v4())
            } else {
                replaced
            }
        }
    };
    if repo::list_projects(&db, &team_key)
        .await
        .iter()
        .any(|p| p.key == project_key)
    {
        return Err(format!("已存在同名项目 {project_key}，请改为导入到现有项目"));
    }
    repo::create_project(&db, &team_key, &project_key, &name).await?;
    let report = imports::import_into_project(&db, &team_key, &project_key, &ifaces).await?;
    Ok((report, name))
}

#[tauri::command]
async fn export_openapi_file(
    state: State<'_, AppState>,
    path: String,
    team_key: String,
    project_key: String,
    yaml: bool,
) -> Result<Vec<String>, String> {
    let db = with_db(&state)?;
    let out = imports::export_openapi(&db, &team_key, &project_key, yaml).await?;
    std::fs::write(&path, out.content).map_err(|e| format!("写入文件失败：{e}"))?;
    Ok(out.warnings)
}

#[tauri::command]
async fn export_interface_openapi_file(
    state: State<'_, AppState>,
    path: String,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    yaml: bool,
) -> Result<Vec<String>, String> {
    let db = with_db(&state)?;
    let out = imports::export_openapi_interface(
        &db,
        &team_key,
        &project_key,
        &group_path,
        &iface_key,
        yaml,
    )
    .await?;
    std::fs::write(&path, out.content).map_err(|e| format!("写入文件失败：{e}"))?;
    Ok(out.warnings)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_session,
            list_teams,
            list_projects,
            create_team,
            create_project,
            delete_team,
            delete_project,
            rename_team,
            rename_project,
            move_interface,
            move_group,
            save_workspace,
            list_interface_tree,
            create_group,
            rename_group,
            delete_group,
            create_interface,
            copy_interface,
            get_interface,
            save_interface,
            rename_interface,
            delete_interface,
            list_environments,
            get_environment,
            save_environment,
            delete_environment,
            set_active_environment,
            get_project_settings,
            save_project_settings,
            send_request,
            list_request_history,
            get_request_history,
            delete_request_history,
            clear_request_history,
            resend_history,
            run_interfaces,
            import_spec_into_project,
            import_spec_new_project,
            export_openapi_file,
            export_interface_openapi_file,
        ])
        .on_page_load(|webview, payload| {
            // 窗口初始为隐藏；首屏加载完成后再显示，避免启动白屏
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                let _ = webview.window().show();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_root_is_home_dot_apidock() {
        // 只断言路径形态，不创建目录
        let root = default_data_root().expect("应能解析用户主目录");
        assert!(root.ends_with(".apidock"), "默认根应为 <home>/.apidock，实际：{}", root.display());
    }
}
