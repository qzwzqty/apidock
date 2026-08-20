mod storage;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager, State};
use notify::Watcher;

use storage::{InterfaceFile, ProjectInfo, TeamInfo, TreeNode, WorkspaceState};

type WatchSender = std::sync::mpsc::Sender<notify::Result<notify::Event>>;
static WATCHER_TX: OnceLock<WatchSender> = OnceLock::new();

pub struct AppState {
    root: Mutex<Option<PathBuf>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self { root: Mutex::new(None), watcher: Mutex::new(None) }
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

#[tauri::command]
fn create_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    key: String,
    name: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    let key = storage::sanitize_key(&key);
    if key.is_empty() {
        return Err("分组键不能为空".into());
    }
    let name = if name.trim().is_empty() { key.clone() } else { name };
    storage::create_group(&root, &team_key, &project_key, &group_path, &key, &name)
}

#[tauri::command]
fn rename_group(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    new_key: String,
    new_name: String,
) -> Result<(), String> {
    let root = with_root(&state)?;
    let new_key = storage::sanitize_key(&new_key);
    if new_key.is_empty() {
        return Err("分组键不能为空".into());
    }
    storage::rename_group(&root, &team_key, &project_key, &group_path, &new_key, &new_name)
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

#[tauri::command]
fn create_interface(
    state: State<'_, AppState>,
    team_key: String,
    project_key: String,
    group_path: Vec<String>,
    key: String,
    name: String,
) -> Result<InterfaceFile, String> {
    let root = with_root(&state)?;
    let key = storage::sanitize_key(&key);
    if key.is_empty() {
        return Err("接口键不能为空".into());
    }
    storage::create_interface(&root, &team_key, &project_key, &group_path, &key, &name)
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
) -> Result<(), String> {
    let root = with_root(&state)?;
    storage::rename_interface(&root, &team_key, &project_key, &group_path, &iface_key, &new_name)
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}