use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 「接口管理」模块目录（数据根目录下与未来其它模块并列）
pub const MODULE_DIR: &str = "api-mgmt";
const WORKSPACE_FILE: &str = "workspace.json";
const TEAM_FILE: &str = "team.json";
const PROJECT_FILE: &str = "project.json";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInfo {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenTab {
    pub team_key: String,
    pub project_key: String,
}

impl OpenTab {
    pub fn id(&self) -> String {
        format!("project:{}:{}", self.team_key, self.project_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub kind: String, // system | custom | none
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceState {
    pub version: u32,
    pub open_tabs: Vec<OpenTab>,
    pub active_tab: Option<String>,
    pub proxy: ProxyConfig,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self { version: SCHEMA_VERSION, open_tabs: Vec::new(), active_tab: None, proxy: ProxyConfig::default() }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamMeta {
    version: u32,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectMeta {
    version: u32,
    name: String,
    #[serde(default)]
    active_environment_id: Option<String>,
    #[serde(default)]
    global_variables: Vec<KeyValue>,
    #[serde(default)]
    global_params: GlobalParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GlobalParams {
    pub headers: Vec<KeyValue>,
    pub cookies: Vec<KeyValue>,
    pub query: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFile {
    pub version: u32,
    pub id: String,
    pub file: String,
    pub name: String,
    pub host: String,
    pub builtin: bool,
    pub variables: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSummary {
    pub id: String,
    pub file: String,
    pub name: String,
    pub host: String,
    pub builtin: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub name: String,
    pub active_environment_id: Option<String>,
    pub global_variables: Vec<KeyValue>,
    pub global_params: GlobalParams,
}

/// 把任意字符串洗成合法目录/文件键（小写字母、数字、连字符，无空格/特殊字符）
pub fn sanitize_key(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in raw.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// 剥离 JSONC 注释（`//`、`/* */`）与尾逗号，保留字符串原样
pub fn strip_jsonc(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    // 最近一个非空白字符在 out 中的字节下标
    let mut last_non_ws: Option<usize> = None;

    // 若最后一个非空白字符是逗号则移除（用于 `,}` / `,]`）
    let trim_trailing_comma = |out: &mut String, last: &mut Option<usize>| {
        if let Some(idx) = *last {
            if out[idx..].starts_with(',') {
                out.truncate(idx);
                // 回退到逗号之前的非空白字符
                *last = out[..idx].char_indices().rev().find(|(_, c)| !c.is_whitespace()).map(|(i, _)| i);
            }
        }
    };

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                out.push(chars.next().unwrap_or('\0'));
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    for n in chars.by_ref() {
                        if n == '\n' {
                            out.push('\n');
                            last_non_ws = None;
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    for n in chars.by_ref() {
                        if prev == '*' && n == '/' {
                            break;
                        }
                        if n == '\n' {
                            out.push('\n');
                            last_non_ws = None;
                        }
                        prev = n;
                    }
                }
                _ => {
                    out.push(c);
                    last_non_ws = Some(out.len() - 1);
                }
            },
            '}' | ']' => {
                trim_trailing_comma(&mut out, &mut last_non_ws);
                out.push(c);
                last_non_ws = Some(out.len() - 1);
            }
            ',' => {
                // 先看后续是否紧跟 } 或 ]（允许空白），是则跳过该逗号
                let mut ahead = chars.clone();
                let mut skip = false;
                while let Some(&n) = ahead.peek() {
                    if n == '}' || n == ']' {
                        skip = true;
                        break;
                    }
                    if n.is_whitespace() {
                        ahead.next();
                    } else {
                        break;
                    }
                }
                if !skip {
                    out.push(c);
                    last_non_ws = Some(out.len() - 1);
                }
            }
            c if c.is_whitespace() => out.push(c),
            c => {
                out.push(c);
                last_non_ws = Some(out.len() - c.len_utf8());
            }
        }
    }
    out
}

fn read_json_text(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    Some(strip_jsonc(&raw))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = read_json_text(path)?;
    serde_json::from_str(&text).ok()
}

/// 原子写：先写临时文件再 rename，避免崩溃/断电损坏数据
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path")
    })?;
    let tmp = dir.join(format!(".tmp-{}", path.file_name().unwrap().to_string_lossy()));
    fs::write(&tmp, content)?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 确保数据根目录与模块目录存在；缺失时创建 workspace.json
pub fn ensure_root(root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(root.join(MODULE_DIR))?;
    let ws = root.join(WORKSPACE_FILE);
    if !ws.exists() {
        let state = serde_json::to_string_pretty(&WorkspaceState::new())
            .map_err(std::io::Error::other)?;
        atomic_write(&ws, &state)?;
    }
    Ok(())
}

pub fn read_workspace(root: &Path) -> WorkspaceState {
    read_json(&root.join(WORKSPACE_FILE)).unwrap_or_else(WorkspaceState::new)
}

pub fn write_workspace(root: &Path, state: &WorkspaceState) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    atomic_write(&root.join(WORKSPACE_FILE), &text)
}

fn module_dir(root: &Path) -> PathBuf {
    root.join(MODULE_DIR)
}

fn team_dir(root: &Path, team_key: &str) -> PathBuf {
    module_dir(root).join(team_key)
}

fn project_dir(root: &Path, team_key: &str, project_key: &str) -> PathBuf {
    team_dir(root, team_key).join(project_key)
}

pub fn list_teams(root: &Path) -> Vec<TeamInfo> {
    let mut teams = Vec::new();
    let Ok(entries) = fs::read_dir(module_dir(root)) else {
        return teams;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let key = entry.file_name().to_string_lossy().into_owned();
        let name = read_json::<TeamMeta>(&entry.path().join(TEAM_FILE))
            .map(|m| m.name)
            .unwrap_or_else(|| key.clone());
        teams.push(TeamInfo { key, name });
    }
    teams.sort_by(|a, b| a.name.cmp(&b.name));
    teams
}

pub fn list_projects(root: &Path, team_key: &str) -> Vec<ProjectInfo> {
    let mut projects = Vec::new();
    let Ok(entries) = fs::read_dir(team_dir(root, team_key)) else {
        return projects;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let key = entry.file_name().to_string_lossy().into_owned();
        let name = read_json::<ProjectMeta>(&entry.path().join(PROJECT_FILE))
            .map(|m| m.name)
            .unwrap_or_else(|| key.clone());
        projects.push(ProjectInfo { key, name });
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
}

pub fn create_team(root: &Path, key: &str, name: &str) -> Result<TeamInfo, String> {
    let dir = team_dir(root, key);
    if dir.exists() {
        return Err(format!("团队键 {key} 已存在"));
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let meta = TeamMeta { version: SCHEMA_VERSION, name: name.to_string() };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    atomic_write(&dir.join(TEAM_FILE), &text).map_err(|e| e.to_string())?;
    Ok(TeamInfo { key: key.to_string(), name: name.to_string() })
}

pub fn create_project(root: &Path, team_key: &str, key: &str, name: &str) -> Result<ProjectInfo, String> {
    let dir = project_dir(root, team_key, key);
    if !team_dir(root, team_key).exists() {
        return Err(format!("团队 {team_key} 不存在"));
    }
    if dir.exists() {
        return Err(format!("项目键 {key} 已存在"));
    }
    fs::create_dir_all(dir.join("api")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("environments")).map_err(|e| e.to_string())?;
    let meta = ProjectMeta {
        version: SCHEMA_VERSION,
        name: name.to_string(),
        active_environment_id: Some("env-prod".into()),
        global_variables: Vec::new(),
        global_params: GlobalParams::default(),
    };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    atomic_write(&dir.join(PROJECT_FILE), &text).map_err(|e| e.to_string())?;
    // 默认三套环境
    for (file_key, env) in default_environments() {
        let f = dir.join("environments").join(format!("{file_key}.json"));
        if !f.exists() {
            let e = serde_json::to_string_pretty(&env).map_err(|e| e.to_string())?;
            atomic_write(&f, &e).map_err(|e| e.to_string())?;
        }
    }
    Ok(ProjectInfo { key: key.to_string(), name: name.to_string() })
}

// ----- 分组 / 接口树 -----

const GROUP_FILE: &str = "group.json";
const INTERFACE_FILE_SUFFIX: &str = ".json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Body {
    pub mode: String, // none | json | raw | urlencoded | form-data | file
    pub content: String,
    pub content_type: String,
    pub form: Vec<KeyValue>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Auth {
    pub kind: String, // none | bearer | basic | api-key
    pub token: String,
    pub username: String,
    pub password: String,
    pub api_key_name: String,
    pub api_key_in: String, // header | query
    pub api_key_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InterfaceFile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query: Vec<KeyValue>,
    pub body: Body,
    pub auth: Auth,
    pub variables: Vec<KeyValue>,
    pub description: String,
    // 发送选项（None 用默认值）
    pub timeout_ms: Option<u64>,
    pub redirect_limit: Option<u64>,
    pub tls_verify: Option<bool>,
    pub ca_cert_path: Option<String>,
}

impl InterfaceFile {
    pub fn new(key: &str) -> Self {
        Self {
            version: SCHEMA_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            name: key.to_string(),
            method: "GET".into(),
            url: String::new(),
            headers: Vec::new(),
            query: Vec::new(),
            body: Body::default(),
            auth: Auth::default(),
            variables: Vec::new(),
            description: String::new(),
            timeout_ms: None,
            redirect_limit: None,
            tls_verify: None,
            ca_cert_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TreeNode {
    #[serde(rename_all = "camelCase")]
    Group { key: String, name: String, children: Vec<TreeNode> },
    #[serde(rename_all = "camelCase")]
    Interface { key: String, name: String, method: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupMeta {
    version: u32,
    name: String,
}

fn default_environments() -> Vec<(&'static str, EnvironmentFile)> {
    vec![
        ("prod", EnvironmentFile { version: SCHEMA_VERSION, id: "env-prod".to_string(), file: "prod".to_string(), name: "正式环境".to_string(), host: String::new(), builtin: true, variables: Vec::new() }),
        ("test", EnvironmentFile { version: SCHEMA_VERSION, id: "env-test".to_string(), file: "test".to_string(), name: "测试环境".to_string(), host: String::new(), builtin: true, variables: Vec::new() }),
        ("dev", EnvironmentFile { version: SCHEMA_VERSION, id: "env-dev".to_string(), file: "dev".to_string(), name: "开发环境".to_string(), host: String::new(), builtin: true, variables: Vec::new() }),
    ]
}

fn project_api_dir(root: &Path, team_key: &str, project_key: &str) -> PathBuf {
    project_dir(root, team_key, project_key).join("api")
}

fn group_dir_at(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
) -> PathBuf {
    let mut dir = project_api_dir(root, team_key, project_key);
    for seg in group_path {
        dir = dir.join(seg);
    }
    dir
}

fn interface_file(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> PathBuf {
    group_dir_at(root, team_key, project_key, group_path).join(format!(
        "{iface_key}{INTERFACE_FILE_SUFFIX}"
    ))
}

fn read_interface(root: &Path, team_key: &str, project_key: &str, group_path: &[String], iface_key: &str) -> Option<InterfaceFile> {
    read_json(&interface_file(root, team_key, project_key, group_path, iface_key))
}

/// 递归扫描接口树
pub fn list_interface_tree(
    root: &Path,
    team_key: &str,
    project_key: &str,
) -> Vec<TreeNode> {
    let api_dir = project_api_dir(root, team_key, project_key);
    scan_tree_dir(&api_dir)
}

fn scan_tree_dir(dir: &Path) -> Vec<TreeNode> {
    let mut nodes = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return nodes;
    };
    let mut items: Vec<(String, PathBuf, bool)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with('~') {
            continue;
        }
        let is_dir = path.is_dir();
        let is_group_file = name == GROUP_FILE;
        if is_dir || (!is_group_file && name.ends_with(INTERFACE_FILE_SUFFIX)) {
            items.push((name, path, is_dir));
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, path, is_dir) in items {
        if is_dir {
            let group_name = read_json::<GroupMeta>(&path.join(GROUP_FILE))
                .map(|m| m.name)
                .unwrap_or_else(|| name.clone());
            nodes.push(TreeNode::Group { key: name, name: group_name, children: scan_tree_dir(&path) });
        } else {
            let iface: InterfaceFile = read_json(&path)
                .unwrap_or_else(|| InterfaceFile::new(&name.trim_end_matches(INTERFACE_FILE_SUFFIX)));
            nodes.push(TreeNode::Interface { key: name.trim_end_matches(INTERFACE_FILE_SUFFIX).to_string(), name: iface.name, method: iface.method });
        }
    }
    nodes
}

/// 分组：找到某 key 在 group_path 下的下一级目录路径（用于在指定分组下新建）
fn project_api_exists(root: &Path, team_key: &str, project_key: &str) -> bool {
    project_api_dir(root, team_key, project_key).is_dir()
}

pub fn create_group(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    key: &str,
    name: &str,
) -> Result<(), String> {
    if !project_api_exists(root, team_key, project_key) {
        return Err(format!("项目 {project_key} 不存在"));
    }
    let parent = group_dir_at(root, team_key, project_key, group_path);
    if !parent.is_dir() {
        return Err("父分组不存在".into());
    }
    let dir = parent.join(key);
    if dir.exists() {
        return Err(format!("分组键 {key} 已存在"));
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let meta = GroupMeta { version: SCHEMA_VERSION, name: name.to_string() };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    atomic_write(&dir.join(GROUP_FILE), &text).map_err(|e| e.to_string())
}

pub fn rename_group(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    new_key: &str,
    new_name: &str,
) -> Result<(), String> {
    let dir = group_dir_at(root, team_key, project_key, group_path);
    if !dir.is_dir() {
        return Err("分组不存在".into());
    }
    if group_path.is_empty() {
        return Err("不能重命名项目根".into());
    }
    let parent = dir.parent().map(Path::to_path_buf).unwrap_or_default();
    if new_key != group_path.last().map(String::as_str).unwrap_or_default() {
        let target = parent.join(new_key);
        if target.exists() {
            return Err(format!("分组键 {new_key} 已存在"));
        }
        fs::rename(&dir, &target).map_err(|e| e.to_string())?;
    }
    let meta = GroupMeta { version: SCHEMA_VERSION, name: new_name.to_string() };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    atomic_write(&parent.join(new_key).join(GROUP_FILE), &text).map_err(|e| e.to_string())
}

pub fn delete_group(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
) -> Result<(), String> {
    let dir = group_dir_at(root, team_key, project_key, group_path);
    if !dir.is_dir() {
        return Err("分组不存在".into());
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

pub fn create_interface(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    key: &str,
    name: &str,
) -> Result<InterfaceFile, String> {
    if !project_api_exists(root, team_key, project_key) {
        return Err(format!("项目 {project_key} 不存在"));
    }
    let parent = group_dir_at(root, team_key, project_key, group_path);
    if !parent.is_dir() {
        return Err("分组不存在".into());
    }
    let path = parent.join(format!("{key}{INTERFACE_FILE_SUFFIX}"));
    if path.exists() {
        return Err(format!("接口键 {key} 已存在"));
    }
    let mut iface = InterfaceFile::new(key);
    if !name.trim().is_empty() {
        iface.name = name.to_string();
    }
    let text = serde_json::to_string_pretty(&iface).map_err(|e| e.to_string())?;
    atomic_write(&path, &text).map_err(|e| e.to_string())?;
    Ok(iface)
}

pub fn get_interface(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> Result<InterfaceFile, String> {
    read_interface(root, team_key, project_key, group_path, iface_key)
        .ok_or_else(|| format!("接口 {iface_key} 不存在"))
}

/// 保存整个接口定义（标准 JSON 写入）
pub fn save_interface(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    iface: &InterfaceFile,
) -> Result<(), String> {
    let path = interface_file(root, team_key, project_key, group_path, iface_key);
    if !path.exists() {
        return Err(format!("接口 {iface_key} 不存在"));
    }
    let text = serde_json::to_string_pretty(iface).map_err(|e| e.to_string())?;
    atomic_write(&path, &text).map_err(|e| e.to_string())
}

/// 重命名接口：仅改 name 字段，不动磁盘文件名
pub fn rename_interface(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    new_name: &str,
) -> Result<(), String> {
    let mut iface = get_interface(root, team_key, project_key, group_path, iface_key)?;
    iface.name = new_name.to_string();
    save_interface(root, team_key, project_key, group_path, iface_key, &iface)
}

pub fn delete_interface(
    root: &Path,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> Result<(), String> {
    let path = interface_file(root, team_key, project_key, group_path, iface_key);
    if !path.exists() {
        return Err(format!("接口 {iface_key} 不存在"));
    }
    fs::remove_file(&path).map_err(|e| e.to_string())
}

// ----- 环境 / 项目设置 -----

fn environments_dir(root: &Path, team_key: &str, project_key: &str) -> PathBuf {
    project_dir(root, team_key, project_key).join("environments")
}

fn project_meta_file(root: &Path, team_key: &str, project_key: &str) -> PathBuf {
    project_dir(root, team_key, project_key).join(PROJECT_FILE)
}

fn read_project_meta(root: &Path, team_key: &str, project_key: &str) -> Option<ProjectMeta> {
    read_json(&project_meta_file(root, team_key, project_key))
}

fn write_project_meta(
    root: &Path,
    team_key: &str,
    project_key: &str,
    meta: &ProjectMeta,
) -> Result<(), String> {
    let text = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    atomic_write(&project_meta_file(root, team_key, project_key), &text).map_err(|e| e.to_string())
}

/// 读取环境文件：按 id 匹配；找不到时也接受按文件名键匹配
pub fn get_environment(
    root: &Path,
    team_key: &str,
    project_key: &str,
    env_id: &str,
) -> Result<EnvironmentFile, String> {
    let dir = environments_dir(root, team_key, project_key);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Err(format!("环境 {env_id} 不存在"));
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|e| e == "json") {
            continue;
        }
        if let Some(env) = read_json::<EnvironmentFile>(&path) {
            if env.id == env_id {
                return Ok(env);
            }
        }
        let stem = path.file_stem().unwrap().to_string_lossy();
        if stem == env_id {
            let stem = stem.into_owned();
            let env = EnvironmentFile {
                version: SCHEMA_VERSION,
                id: env_id.to_string(),
                file: stem.clone(),
                name: stem,
                host: String::new(),
                builtin: false,
                variables: Vec::new(),
            };
            return Ok(env);
        }
    }
    Err(format!("环境 {env_id} 不存在"))
}

pub fn list_environments(
    root: &Path,
    team_key: &str,
    project_key: &str,
) -> Vec<EnvironmentSummary> {
    let active = read_project_meta(root, team_key, project_key)
        .and_then(|m| m.active_environment_id);
    let dir = environments_dir(root, team_key, project_key);
    let mut list = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return list;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|e| e == "json") {
            continue;
        }
        let file = path.file_stem().unwrap().to_string_lossy().into_owned();
        if let Some(env) = read_json::<EnvironmentFile>(&path) {
            let id = env.id;
            let is_active = Some(id.clone()) == active;
            list.push(EnvironmentSummary {
                id,
                file,
                name: env.name,
                host: env.host,
                builtin: env.builtin,
                active: is_active,
            });
        }
    }
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

pub fn save_environment(
    root: &Path,
    team_key: &str,
    project_key: &str,
    env: EnvironmentFile,
) -> Result<(), String> {
    let dir = environments_dir(root, team_key, project_key);
    let target = dir.join(format!("{}.json", env.file));
    // 若被重命名到已存在的其它环境文件键，冲突时拒绝
    if target.exists()
        && read_json::<EnvironmentFile>(&target)
            .map(|e| e.id != env.id)
            .unwrap_or(false)
    {
        return Err(format!("环境文件键 {} 已被占用", env.file));
    }
    let text = serde_json::to_string_pretty(&env).map_err(|e| e.to_string())?;
    atomic_write(&target, &text).map_err(|e| e.to_string())
}

pub fn delete_environment(
    root: &Path,
    team_key: &str,
    project_key: &str,
    env_id: &str,
) -> Result<(), String> {
    let dir = environments_dir(root, team_key, project_key);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Err(format!("环境 {env_id} 不存在"));
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|e| e == "json") {
            continue;
        }
        if let Some(env) = read_json::<EnvironmentFile>(&path) {
            if env.id == env_id {
                if env.builtin {
                    return Err("内置环境不可删除".into());
                }
                return fs::remove_file(&path).map_err(|e| e.to_string());
            }
        }
    }
    Err(format!("环境 {env_id} 不存在"))
}

pub fn set_active_environment(
    root: &Path,
    team_key: &str,
    project_key: &str,
    env_id: &str,
) -> Result<(), String> {
    get_environment(root, team_key, project_key, env_id)?;
    let mut meta = read_project_meta(root, team_key, project_key).ok_or("项目不存在")?;
    meta.active_environment_id = Some(env_id.to_string());
    write_project_meta(root, team_key, project_key, &meta)
}

pub fn get_project_settings(
    root: &Path,
    team_key: &str,
    project_key: &str,
) -> Result<ProjectSettings, String> {
    let meta = read_project_meta(root, team_key, project_key).ok_or("项目不存在")?;
    Ok(ProjectSettings {
        name: meta.name,
        active_environment_id: meta.active_environment_id,
        global_variables: meta.global_variables,
        global_params: meta.global_params,
    })
}

pub fn save_project_settings(
    root: &Path,
    team_key: &str,
    project_key: &str,
    settings: ProjectSettings,
) -> Result<(), String> {
    let mut meta = read_project_meta(root, team_key, project_key).ok_or("项目不存在")?;
    meta.name = settings.name;
    meta.global_variables = settings.global_variables;
    meta.global_params = settings.global_params;
    write_project_meta(root, team_key, project_key, &meta)
}

pub fn delete_team(root: &Path, team_key: &str) -> Result<(), String> {
    let dir = team_dir(root, team_key);
    if !dir.exists() {
        return Err(format!("团队 {team_key} 不存在"));
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

pub fn delete_project(root: &Path, team_key: &str, project_key: &str) -> Result<(), String> {
    let dir = project_dir(root, team_key, project_key);
    if !dir.exists() {
        return Err(format!("项目 {project_key} 不存在"));
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "apidock-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn setup(root: &Path) {
        ensure_root(root).unwrap();
        create_team(root, "ops", "运维团队").unwrap();
        create_project(root, "ops", "user-api", "用户中心").unwrap();
    }

    #[test]
    fn sanitize_key_keeps_ascii_dashes() {
        assert_eq!(sanitize_key("Order Service"), "order-service");
        assert_eq!(sanitize_key("  API--Docs  "), "api-docs");
        assert_eq!(sanitize_key("中文团队"), "");
        assert_eq!(sanitize_key("Login_v2!"), "login-v2");
    }

    #[test]
    fn strip_jsonc_handles_comments_and_trailing_comma() {
        let src = r#"{
  // 行注释
  "name": "测试 /* 字符串内不减 */",
  "list": [1, 2, 3,],  /* 块注释 */
}"#;
        let text = strip_jsonc(src);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["name"], "测试 /* 字符串内不减 */");
        assert_eq!(v["list"][2], 3);
    }

    #[test]
    fn team_and_project_crud() {
        let root = temp_root();
        ensure_root(&root).unwrap();

        let team = create_team(&root, "ops", "运维团队").unwrap();
        assert_eq!(team.name, "运维团队");
        assert!(create_team(&root, "ops", "重复").is_err());

        let proj = create_project(&root, "ops", "user-api", "用户中心").unwrap();
        assert_eq!(proj.name, "用户中心");
        assert!(create_project(&root, "ops", "user-api", "x").is_err());
        assert!(create_project(&root, "missing", "x", "y").is_err());

        assert_eq!(list_teams(&root).len(), 1);
        assert_eq!(list_projects(&root, "ops").len(), 1);

        // 带注释的手写 team.json 仍可被读取
        let team_json = root.join(MODULE_DIR).join("ops").join(TEAM_FILE);
        std::fs::write(
            &team_json,
            r#"{
  // 手工注释
  "version": 1,
  "name": "运维团队-改",
}"#,
        )
        .unwrap();
        let teams = list_teams(&root);
        assert_eq!(teams[0].name, "运维团队-改");

        // workspace 标签状态持久化
        let ws = WorkspaceState {
            version: 1,
            open_tabs: vec![OpenTab { team_key: "ops".into(), project_key: "user-api".into() }],
            active_tab: Some("project:ops:user-api".into()),
            proxy: ProxyConfig::default(),
        };
        write_workspace(&root, &ws).unwrap();
        assert_eq!(read_workspace(&root).open_tabs.len(), 1);

        delete_project(&root, "ops", "user-api").unwrap();
        delete_team(&root, "ops").unwrap();
        assert!(list_teams(&root).is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn project_creates_default_environments() {
        let root = temp_root();
        setup(&root);
        let env_dir = project_dir(&root, "ops", "user-api").join("environments");
        assert!(env_dir.join("prod.json").exists());
        assert!(env_dir.join("test.json").exists());
        assert!(env_dir.join("dev.json").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn group_and_interface_tree_crud() {
        let root = temp_root();
        setup(&root);

        // 建多级分组
        create_group(&root, "ops", "user-api", &[], "auth", "鉴权").unwrap();
        create_group(&root, "ops", "user-api", &["auth".to_string()], "login", "登录").unwrap();
        assert!(create_group(&root, "ops", "user-api", &[], "auth", "x").is_err());

        // 根级与分组级接口
        let iface = create_interface(&root, "ops", "user-api", &[], "health", "健康检查").unwrap();
        assert_eq!(iface.method, "GET");
        assert_eq!(iface.name, "健康检查");
        create_interface(&root, "ops", "user-api", &["auth".to_string(), "login".to_string()], "do-login", "登录接口").unwrap();
        assert!(create_interface(&root, "ops", "user-api", &[], "health", "x").is_err());

        // 树（按键字母序：auth 分组在前，health 接口在后）
        let tree = list_interface_tree(&root, "ops", "user-api");
        assert_eq!(tree.len(), 2);
        match &tree[0] {
            TreeNode::Group { key, name, children } => {
                assert_eq!(key, "auth");
                assert_eq!(name, "鉴权");
                assert_eq!(children.len(), 1);
                match &children[0] {
                    TreeNode::Group { name, .. } => assert_eq!(name, "登录"),
                    _ => panic!("expected login group"),
                }
            }
            _ => panic!("expected auth group"),
        }
        assert!(matches!(&tree[1], TreeNode::Interface { key, .. } if key == "health"));

        // 编辑（JSONC 宽容：给文件加注释仍可读）
        let iface_path = interface_file(&root, "ops", "user-api", &[], "health");
        std::fs::write(
            &iface_path,
            r#"{
  "version": 1, /* 注释 */
  "id": "x",
  "name": "健康检查改",
  "method": "POST",
  "url": "/ping",
  "headers": [], "query": [],
  "description": "",
}"#,
        )
        .unwrap();
        let got = get_interface(&root, "ops", "user-api", &[], "health").unwrap();
        assert_eq!(got.name, "健康检查改");
        assert_eq!(got.method, "POST");
        assert_eq!(got.url, "/ping");

        // 保存 / 重命名
        let mut doc = got.clone();
        doc.description = "说明".into();
        save_interface(&root, "ops", "user-api", &[], "health", &doc).unwrap();
        rename_interface(&root, "ops", "user-api", &[], "health", "健康检查v2").unwrap();
        assert_eq!(get_interface(&root, "ops", "user-api", &[], "health").unwrap().name, "健康检查v2");
        assert!(interface_file(&root, "ops", "user-api", &[], "health").exists());

        // 重命名分组（改键会移动目录）
        rename_group(&root, "ops", "user-api", &["auth".to_string()], "auth2", "鉴权v2").unwrap();
        assert!(group_dir_at(&root, "ops", "user-api", &["auth2".to_string()]).is_dir());
        assert!(!group_dir_at(&root, "ops", "user-api", &["auth".to_string()]).is_dir());

        // 删除
        delete_interface(&root, "ops", "user-api", &[], "health").unwrap();
        delete_group(&root, "ops", "user-api", &["auth2".to_string()]).unwrap();
        assert!(list_interface_tree(&root, "ops", "user-api").is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn environment_lifecycle() {
        let root = temp_root();
        setup(&root);

        let list = list_environments(&root, "ops", "user-api");
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|e| e.id == "env-prod" && e.active));

        // 激活其它环境
        set_active_environment(&root, "ops", "user-api", "env-dev").unwrap();
        let list = list_environments(&root, "ops", "user-api");
        assert!(list.iter().any(|e| e.id == "env-dev" && e.active));

        // 编辑环境（host + 变量）
        let mut dev = get_environment(&root, "ops", "user-api", "env-dev").unwrap();
        dev.host = "https://dev.example.com".into();
        dev.variables.push(KeyValue { key: "token".into(), value: "abc".into(), enabled: true });
        save_environment(&root, "ops", "user-api", dev).unwrap();
        let dev2 = get_environment(&root, "ops", "user-api", "env-dev").unwrap();
        assert_eq!(dev2.host, "https://dev.example.com");
        assert_eq!(dev2.variables[0].value, "abc");

        // 新增自定义环境
        let custom = EnvironmentFile {
            version: 1,
            id: "env-staging".into(),
            file: "staging".into(),
            name: "预发布".into(),
            host: String::new(),
            builtin: false,
            variables: Vec::new(),
        };
        save_environment(&root, "ops", "user-api", custom).unwrap();
        assert_eq!(list_environments(&root, "ops", "user-api").len(), 4);

        // 内置不可删，自定义可删
        assert!(delete_environment(&root, "ops", "user-api", "env-prod").is_err());
        delete_environment(&root, "ops", "user-api", "env-staging").unwrap();
        assert_eq!(list_environments(&root, "ops", "user-api").len(), 3);

        // 项目设置（全局变量/参数）
        let mut settings = get_project_settings(&root, "ops", "user-api").unwrap();
        assert_eq!(settings.active_environment_id.as_deref(), Some("env-dev"));
        settings.global_variables.push(KeyValue { key: "host".into(), value: "http://glob.example.com".into(), enabled: true });
        settings.global_params.headers.push(KeyValue { key: "X-Trace".into(), value: "1".into(), enabled: true });
        save_project_settings(&root, "ops", "user-api", settings).unwrap();
        let s2 = get_project_settings(&root, "ops", "user-api").unwrap();
        assert_eq!(s2.global_variables[0].key, "host");
        assert_eq!(s2.global_params.headers[0].key, "X-Trace");

        std::fs::remove_dir_all(&root).unwrap();
    }
}