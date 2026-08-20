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
pub struct WorkspaceState {
    pub version: u32,
    pub open_tabs: Vec<OpenTab>,
    pub active_tab: Option<String>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self { version: SCHEMA_VERSION, open_tabs: Vec::new(), active_tab: None }
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
    let meta = ProjectMeta { version: SCHEMA_VERSION, name: name.to_string() };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    atomic_write(&dir.join(PROJECT_FILE), &text).map_err(|e| e.to_string())?;
    Ok(ProjectInfo { key: key.to_string(), name: name.to_string() })
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
        let team_json = root
            .join(MODULE_DIR)
            .join("ops")
            .join(TEAM_FILE);
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
        };
        write_workspace(&root, &ws).unwrap();
        assert_eq!(read_workspace(&root).open_tabs.len(), 1);

        delete_project(&root, "ops", "user-api").unwrap();
        delete_team(&root, "ops").unwrap();
        assert!(list_teams(&root).is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }
}