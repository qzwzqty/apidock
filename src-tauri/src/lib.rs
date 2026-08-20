mod assertions;
mod http;
mod imports;
mod runner;
mod storage;
mod variables;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager, State};
use notify::Watcher;

use storage::{EnvironmentFile, InterfaceFile, ProjectInfo, ProjectSettings, TeamInfo, TreeNode, WorkspaceState};

type WatchSender = std::sync::mpsc::Sender<notify::Result<notify::Event>>;
static WATCHER_TX: OnceLock<WatchSender> = OnceLock::new();

pub struct AppState {
    root: Mutex<Option<PathBuf>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    cookiejar: std::sync::Arc<reqwest::cookie::Jar>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            root: Mutex::new(None),
            watcher: Mutex::new(None),
            cookiejar: std::sync::Arc::new(reqwest::cookie::Jar::default()),
        }
    }
}

/// 一次会话的初始快照：数据根目录 + 团队列表 + 标签栏状态
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSession {
    pub data_root: Option<String>,
    pub teams: Vec<TeamInfo>,
    pub workspace: WorkspaceState,
}

#[tauri::command]
fn get_session(state: State<'_, AppState>) -> Result<AppSession, String> {
    let root = restore_root(&state);
    build_session(root.as_deref())
}

#[tauri::command]
fn set_data_root(state: State<'_, AppState>, path: String) -> Result<AppSession, String> {
    let root = PathBuf::from(path);
    storage::ensure_root(&root).map_err(|e| e.to_string())?;
    persist_root(&state, &root)?;
    {
        let mut guard = state.root.lock().unwrap();
        *guard = Some(root.clone());
    }
    restart_watcher(&state, &root)?;
    build_session(Some(&root))
}

#[tauri::command]
fn get_data_root(state: tauri::State<'_, AppState>) -> Option<String> {
    restore_root(&state).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn list_teams(state: tauri::State<'_, AppState>) -> Result<Vec<TeamInfo>, String> {
    let root = with_root(&state)?;
    Ok(storage::list_teams(&root))
}

#[tauri::command]
fn list_projects<'r>(
    state: tauri::State<'r, AppState>,
    team_key: String,
) -> Result<Vec<ProjectInfo>, String> {
    let root = with_root(&state)?;
    Ok(storage::list_projects(&root, &team_key))
}

#[tauri::command]
fn create_team(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<TeamInfo, String> {
    let root = with_root(&state)?;
    // 目录名 = 用户输入名（只校验特殊字符）
    let name = storage::validate_name(&name)?;
    if storage::list_teams(&root).iter().any(|t| t.key == name) {
        return Err("已存在同名团队".into());
    }
    let team = storage::create_team(&root, &name, &name)?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            storage::set_team_description(&root, &name, desc.trim())?;
        }
    }
    Ok(team)
}

#[tauri::command]
fn create_project(
    state: State<'_, AppState>,
    team_key: String,
    name: String,
    description: Option<String>,
) -> Result<ProjectInfo, String> {
    let root = with_root(&state)?;
    let name = storage::validate_name(&name)?;
    if storage::list_projects(&root, &team_key).iter().any(|p| p.key == name) {
        return Err("已存在同名项目".into());
    }
    let project = storage::create_project(&root, &team_key, &name, &name)?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            storage::set_project_description(&root, &team_key, &name, desc.trim())?;
        }
    }
    Ok(project)
}

#[tauri::command]
fn delete_team(state: tauri::State<'_, AppState>, team_key: String) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_team(&root, &team_key)
}

#[tauri::command]
fn delete_project(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_project(&root, &team_key, &project_key)
}

#[tauri::command]
fn rename_team(
    state: State<'_, AppState>,
    team_key: String,
    new_name: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    let new_name = storage::validate_name(&new_name)?;
    if new_name != team_key && storage::list_teams(&root).iter().any(|t| t.key == new_name) {
        return Err("已存在同名团队".into());
    }
    storage::rename_team(&root, &team_key, &new_name)
}

#[tauri::command]
fn rename_project(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    new_name: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    let new_name = storage::validate_name(&new_name)?;
    if new_name != project_key && storage::list_projects(&root, &team_key).iter().any(|p| p.key == new_name) {
        return Err("已存在同名项目".into());
    }
    storage::rename_project(&root, &team_key, &project_key, &new_name)
}

#[tauri::command]
fn move_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    target_group_path: Vec<String>,
) -> Result<String, String> {
    let root = with_root(&state)?;
    storage::move_interface(&root, &team_key, &project_key, &group_path, &iface_key, &target_group_path)
}

#[tauri::command]
fn move_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    target_group_path: Vec<String>,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::move_group(&root, &team_key, &project_key, &group_path, &target_group_path)
}

#[tauri::command]
fn save_workspace(
    state: State<'_, AppState>,
    workspace: WorkspaceState,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::write_workspace(&root, &workspace).map_err(|e| e.to_string())
}

// ----- 接口 / 分组树 -----

#[tauri::command]
fn list_interface_tree(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<Vec<TreeNode>, String> {
    let root = with_root(&state)?;
    Ok(storage::list_interface_tree(&root, &team_key, &project_key))
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
fn create_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    let root = with_root(&state)?;
    let name = storage::validate_name(&name)?;
    let keys = dir_keys(&storage::list_interface_tree(&root, &team_key, &project_key), &group_path);
    if keys.contains(&name) {
        return Err("已存在同名分组/接口".into());
    }
    storage::create_group(&root, &team_key, &project_key, &group_path, &name, &name)?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            storage::set_group_description(&root, &team_key, &project_key, &group_path, desc.trim())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn rename_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    new_name: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    let new_name = storage::validate_name(&new_name)?;
    let keys = dir_keys(&storage::list_interface_tree(&root, &team_key, &project_key), &group_path[..group_path.len().saturating_sub(1)]);
    if keys.iter().any(|k| k == &new_name && k != group_path.last().map(String::as_str).unwrap_or_default()) {
        return Err("已存在同名分组/接口".into());
    }
    storage::rename_group(&root, &team_key, &project_key, &group_path, &new_name)
}

#[tauri::command]
fn delete_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_group(&root, &team_key, &project_key, &group_path)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedInterface {
    key: String,
    file: InterfaceFile,
}

#[tauri::command]
fn create_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    name: String,
    description: Option<String>,
) -> Result<CreatedInterface, String> {
    let root = with_root(&state)?;
    let name = storage::validate_name(&name)?;
    let keys = dir_keys(&storage::list_interface_tree(&root, &team_key, &project_key), &group_path);
    if keys.contains(&name) {
        return Err("已存在同名分组/接口".into());
    }
    let mut iface = storage::create_interface(&root, &team_key, &project_key, &group_path, &name, &name)?;
    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            iface.description = desc.trim().to_string();
            storage::save_interface(&root, &team_key, &project_key, &group_path, &name, &iface)?;
        }
    }
    Ok(CreatedInterface { key: name, file: iface })
}

#[tauri::command]
fn get_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
) -> Result<InterfaceFile, String> {
    let root = with_root(&state)?;
    storage::get_interface(&root, &team_key, &project_key, &group_path, &iface_key)
}

#[tauri::command]
fn save_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    iface: InterfaceFile,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::save_interface(&root, &team_key, &project_key, &group_path, &iface_key, &iface)
}

#[tauri::command]
fn rename_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
    new_name: String,
) -> Result<String, String> {
    let root = with_root(&state)?;
    let new_name = storage::validate_name(&new_name)?;
    if new_name != iface_key {
        let keys = dir_keys(&storage::list_interface_tree(&root, &team_key, &project_key), &group_path);
        if keys.iter().any(|k| k == &new_name) {
            return Err("已存在同名分组/接口".into());
        }
    }
    storage::rename_interface(&root, &team_key, &project_key, &group_path, &iface_key, &new_name)?;
    Ok(new_name)
}

#[tauri::command]
fn delete_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    iface_key: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_interface(&root, &team_key, &project_key, &group_path, &iface_key)
}

// ----- 环境 / 项目设置 -----

#[tauri::command]
fn list_environments(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<Vec<storage::EnvironmentSummary>, String> {
    let root = with_root(&state)?;
    Ok(storage::list_environments(&root, &team_key, &project_key))
}

#[tauri::command]
fn get_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
) -> Result<EnvironmentFile, String> {
    let root = with_root(&state)?;
    storage::get_environment(&root, &team_key, &project_key, &env_id)
}

#[tauri::command]
fn save_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env: EnvironmentFile,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::save_environment(&root, &team_key, &project_key, env)
}

#[tauri::command]
fn delete_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_environment(&root, &team_key, &project_key, &env_id)
}

#[tauri::command]
fn set_active_environment(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::set_active_environment(&root, &team_key, &project_key, &env_id)
}

#[tauri::command]
fn get_project_settings(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<ProjectSettings, String> {
    let root = with_root(&state)?;
    storage::get_project_settings(&root, &team_key, &project_key)
}

#[tauri::command]
fn save_project_settings(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    settings: ProjectSettings,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::save_project_settings(&root, &team_key, &project_key, settings)
}

// ----- 发送请求 -----

#[tauri::command]
async fn send_request(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    env_id: String,
    iface: InterfaceFile,
) -> Result<http::SendResponse, http::SendErrorInfo> {
    let root = {
        let guard = state.root.lock().unwrap();
        guard
            .clone()
            .ok_or_else(|| http::SendErrorInfo {
                kind: "http".into(),
                message: "尚未选择数据根目录".into(),
            })?
    };
    let env = storage::get_environment(&root, &team_key, &project_key, &env_id).map_err(|e| {
        http::SendErrorInfo { kind: "http".into(), message: e }
    })?;
    let settings = storage::get_project_settings(&root, &team_key, &project_key).map_err(|e| {
        http::SendErrorInfo { kind: "http".into(), message: e }
    })?;
    let opts = http::SendOptions {
        proxy: storage::read_workspace(&root).proxy,
        cookie_jar: {
            let jar = state.cookiejar.clone();
            Some(jar)
        },
        ..Default::default()
    };
    http::send(
        &iface,
        &env,
        &settings.global_variables,
        &settings.global_params,
        &opts,
    )
    .await
}

#[tauri::command]
async fn run_interfaces(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
) -> Result<runner::RunReport, String> {
    let root = {
        let guard = state.root.lock().unwrap();
        guard.clone().ok_or("尚未选择数据根目录")?
    };
    runner::run_project(&root, &team_key, &project_key, Some(&group_path).filter(|g| !g.is_empty()).map(|x| x.as_slice())).await
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
    let root = {
        let guard = state.root.lock().unwrap();
        guard.clone().ok_or("尚未选择数据根目录")?
    };
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{e}"))?;
    let (kind, is_yaml) = detect_spec_kind(&content);
    let (name, ifaces) = match kind {
        "postman" => imports::parse_postman(&content)?,
        _ => imports::parse_openapi(&content, is_yaml)?,
    };
    let (report, name) = match imports::import_into_project(&root, &team_key, &project_key, &ifaces) {
        Ok(r) => (r, name),
        Err(e) => return Err(e),
    };
    Ok((report, name))
}

#[tauri::command]
async fn import_spec_new_project(
    state: State<'_, AppState>,
    path: String,
    team_key: String,
) -> Result<(imports::ImportReport, String), String> {
    let root = {
        let guard = state.root.lock().unwrap();
        guard.clone().ok_or("尚未选择数据根目录")?
    };
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{e}"))?;
    let (kind, is_yaml) = detect_spec_kind(&content);
    let (name, ifaces) = match kind {
        "postman" => imports::parse_postman(&content)?,
        _ => imports::parse_openapi(&content, is_yaml)?,
    };
    let project_key = match storage::validate_name(&name) {
        Ok(k) => k,
        Err(_) => {
            let replaced: String = name
                .trim()
                .chars()
                .map(|c| {
                    if c <= '\u{1f}' || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                        '-'
                    } else {
                        c
                    }
                })
                .collect();
            let replaced = replaced.trim_end_matches(|c| c == '.' || c == '-' || c == ' ').to_string();
            if replaced.is_empty() {
                format!("imported-{}", uuid::Uuid::new_v4())
            } else {
                replaced
            }
        }
    };
    if storage::list_projects(&root, &team_key).iter().any(|p| p.key == project_key) {
        return Err(format!("已存在同名项目 {project_key}，请改为导入到现有项目"));
    }
    storage::create_project(&root, &team_key, &project_key, &name)?;
    let report = imports::import_into_project(&root, &team_key, &project_key, &ifaces)?;
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
    let root = {
        let guard = state.root.lock().unwrap();
        guard.clone().ok_or("尚未选择数据根目录")?
    };
    let out = imports::export_openapi(&root, &team_key, &project_key, yaml)?;
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
    let root = {
        let guard = state.root.lock().unwrap();
        guard.clone().ok_or("尚未选择数据根目录")?
    };
    let out = imports::export_openapi_interface(&root, &team_key, &project_key, &group_path, &iface_key, yaml)?;
    std::fs::write(&path, out.content).map_err(|e| format!("写入文件失败：{e}"))?;
    Ok(out.warnings)
}

fn restart_watcher(state: &AppState, root: &PathBuf) -> Result<(), String> {
    let mut guard = state.watcher.lock().unwrap();
    *guard = Some(start_watcher(root)?);
    Ok(())
}

fn start_watcher(root: &PathBuf) -> Result<notify::RecommendedWatcher, String> {
    let sender = WATCHER_TX
        .get()
        .ok_or("watcher channel not ready")?
        .clone();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = sender.send(res);
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(root, notify::RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    Ok(watcher)
}

fn build_session(root: Option<&std::path::Path>) -> Result<AppSession, String> {
    let (data_root, teams, workspace) = match root {
        Some(r) => (
            Some(r.to_string_lossy().into_owned()),
            storage::list_teams(r),
            storage::read_workspace(r),
        ),
        None => (None, Vec::new(), WorkspaceState::new()),
    };
    Ok(AppSession { data_root, teams, workspace })
}

fn with_root(state: &AppState) -> Result<PathBuf, String> {
    let guard = state.root.lock().unwrap();
    guard
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "尚未选择数据根目录".to_string())
}

/// 取当前根目录；若为空则尝试从应用配置目录恢复上次选择
fn restore_root(state: &AppState) -> Option<PathBuf> {
    {
        let mut guard = state.root.lock().unwrap();
        if let Some(root) = guard.clone() {
            if root.is_dir() {
                return Some(root);
            }
        }
        // 回落：从最近记录恢复
        let recent = match std::fs::read_to_string(recent_root_path_file()) {
            Ok(text) => {
                let buf = PathBuf::from(text.trim());
                buf.is_dir().then_some(buf)
            }
            Err(_) => None,
        };
        if let Some(path) = recent {
            *guard = Some(path.clone());
            return Some(path);
        }
        None
    }
}

fn recent_root_path_file() -> PathBuf {
    let dir = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("apidock");
    dir.join("recent-data-root.txt")
}

fn persist_root(_state: &AppState, root: &PathBuf) -> Result<(), String> {
    let file = recent_root_path_file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, root.to_string_lossy().as_ref()).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let _ = WATCHER_TX.set(tx);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_session,
            set_data_root,
            get_data_root,
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
            run_interfaces,
            import_spec_into_project,
            import_spec_new_project,
            export_openapi_file,
            export_interface_openapi_file,
        ])
        .setup(move |app| {
            // 文件变更去抖后 emit，供前端刷新
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut pending = false;
                loop {
                    match rx.recv_timeout(std::time::Duration::from_millis(300)) {
                        Ok(Ok(_evt)) => pending = true,
                        Ok(Err(_)) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if pending {
                                let _ = handle.emit("fs://changed", ());
                                pending = false;
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });

            // 启动即恢复上次的数据根目录并开始文件监听
            let state = app.state::<AppState>();
            if let Some(root) = restore_root(&state) {
                let _ = restart_watcher(&state, &root);
            }
            Ok(())
        })
        .on_page_load(|webview, payload| {
            // 窗口初始为隐藏；首屏加载完成后再显示，避免启动白屏
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                let _ = webview.window().show();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}