mod storage;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

use storage::{ProjectInfo, TeamInfo, WorkspaceState};

pub struct AppState {
    root: Mutex<Option<PathBuf>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self { root: Mutex::new(None) }
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
fn get_session(state: tauri::State<'_, AppState>) -> Result<AppSession, String> {
    let root = restore_root(&state);
    build_session(root.as_deref())
}

#[tauri::command]
fn set_data_root(state: tauri::State<'_, AppState>, path: String) -> Result<AppSession, String> {
    let root = PathBuf::from(path);
    storage::ensure_root(&root).map_err(|e| e.to_string())?;
    persist_root(&state, &root)?;
    let mut guard = state.root.lock().unwrap();
    *guard = Some(root.clone());
    drop(guard);
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
    state: tauri::State<'_, AppState>,
    key: String,
    name: String,
) -> Result<TeamInfo, String> {
    let root = with_root(&state)?;
    let key = storage::sanitize_key(&key);
    if key.is_empty() {
        return Err("团队键不能为空".into());
    }
    let name = if name.trim().is_empty() { key.clone() } else { name };
    storage::create_team(&root, &key, &name)
}

#[tauri::command]
fn create_project(
    state: tauri::State<'_, AppState>,
    team_key: String,
    key: String,
    name: String,
) -> Result<ProjectInfo, String> {
    let root = with_root(&state)?;
    let key = storage::sanitize_key(&key);
    if key.is_empty() {
        return Err("项目键不能为空".into());
    }
    let name = if name.trim().is_empty() { key.clone() } else { name };
    storage::create_project(&root, &team_key, &key, &name)
}

#[tauri::command]
fn delete_team(state: tauri::State<'_, AppState>, team_key: String) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_team(&root, &team_key)
}

#[tauri::command]
fn delete_project(
    state: tauri::State<'_, AppState>,
    team_key: String,
    project_key: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::delete_project(&root, &team_key, &project_key)
}

#[tauri::command]
fn save_workspace(
    state: tauri::State<'_, AppState>,
    workspace: WorkspaceState,
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::write_workspace(&root, &workspace).map_err(|e| e.to_string())
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
            save_workspace,
        ])
        .setup(|app| {
            // 启动即恢复上次的数据根目录
            let state = app.state::<AppState>();
            let _ = restore_root(&state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}