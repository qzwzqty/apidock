//! 旧版文件存储（ADR-0002）的一次性迁移导入：
//! 扫描数据根目录内的 `workspace.json` + `api-mgmt/` 文件树，
//! 在单个事务内写入 SQLite；成功后把旧文件归档到 `.file-storage-backup-<时间戳>/`。

use crate::db::entity::{group, iface, project, team, workspace};
use crate::db::repo;
use crate::domain::*;
use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const MODULE_DIR: &str = "api-mgmt";
const WORKSPACE_FILE: &str = "workspace.json";
const TEAM_FILE: &str = "team.json";
const PROJECT_FILE: &str = "project.json";
const GROUP_FILE: &str = "group.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamMeta {
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectMeta {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    active_environment_id: Option<String>,
    #[serde(default)]
    global_variables: Vec<KeyValue>,
    #[serde(default)]
    global_params: GlobalParams,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupMeta {
    name: String,
    #[serde(default)]
    description: String,
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

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc(&raw)).ok()
}

/// 根目录内是否存在旧版文件数据
pub fn has_legacy_data(root: &Path) -> bool {
    root.join(MODULE_DIR).is_dir() || root.join(WORKSPACE_FILE).is_file()
}

/// 执行一次性迁移：事务写入数据库，成功后归档旧文件
pub async fn import_legacy(db: &DatabaseConnection, root: &Path) -> Result<(), String> {
    let txn = db.begin().await.map_err(|e| format!("开启迁移事务失败：{e}"))?;

    // 1. workspace.json
    if let Some(ws) = read_json::<WorkspaceState>(&root.join(WORKSPACE_FILE)) {
        let open_tabs = serde_json::to_string(&ws.open_tabs).map_err(|e| e.to_string())?;
        let proxy = serde_json::to_string(&ws.proxy).map_err(|e| e.to_string())?;
        workspace::Entity::insert(workspace::ActiveModel {
            id: Set(1),
            open_tabs: Set(open_tabs),
            active_tab: Set(ws.active_tab.clone()),
            proxy: Set(proxy),
        })
        .on_conflict(
            OnConflict::column(workspace::Column::Id)
                .update_columns([
                    workspace::Column::OpenTabs,
                    workspace::Column::ActiveTab,
                    workspace::Column::Proxy,
                ])
                .to_owned(),
        )
        .exec(&txn)
        .await
        .map_err(|e| format!("迁移工作区状态失败：{e}"))?;
    }

    // 2. 团队 / 项目 / 分组 / 接口 / 环境
    let module_dir = root.join(MODULE_DIR);
    if module_dir.is_dir() {
        let entries = fs::read_dir(&module_dir).map_err(|e| format!("读取模块目录失败：{e}"))?;
        for entry in entries.flatten() {
            let team_dir = entry.path();
            if !team_dir.is_dir() {
                continue;
            }
            let team_key = entry.file_name().to_string_lossy().into_owned();
            let meta = read_json::<TeamMeta>(&team_dir.join(TEAM_FILE)).unwrap_or(TeamMeta {
                name: team_key.clone(),
                description: String::new(),
            });
            let t = team::ActiveModel {
                key: Set(team_key.clone()),
                name: Set(meta.name),
                description: Set(meta.description),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .map_err(|e| format!("迁移团队 {team_key} 失败：{e}"))?;

            let Ok(projects) = fs::read_dir(&team_dir) else { continue };
            for pentry in projects.flatten() {
                let project_dir = pentry.path();
                if !project_dir.is_dir() {
                    continue;
                }
                let project_key = pentry.file_name().to_string_lossy().into_owned();
                import_project(&txn, t.id, &project_dir, &project_key).await?;
            }
        }
    }

    txn.commit().await.map_err(|e| format!("迁移提交失败：{e}"))?;

    // 3. 归档旧文件（不删除，保留在备份目录中）
    archive_legacy_files(root);
    Ok(())
}

async fn import_project(
    txn: &sea_orm::DatabaseTransaction,
    team_id: i32,
    project_dir: &Path,
    project_key: &str,
) -> Result<(), String> {
    let meta = read_json::<ProjectMeta>(&project_dir.join(PROJECT_FILE)).unwrap_or(ProjectMeta {
        name: project_key.to_string(),
        description: String::new(),
        active_environment_id: Some("env-prod".into()),
        global_variables: Vec::new(),
        global_params: GlobalParams::default(),
    });
    let p = project::ActiveModel {
        team_id: Set(team_id),
        key: Set(project_key.to_string()),
        name: Set(meta.name),
        description: Set(meta.description),
        active_environment_id: Set(meta.active_environment_id),
        global_variables: Set(serde_json::to_string(&meta.global_variables).unwrap_or_else(|_| "[]".into())),
        global_params: Set(serde_json::to_string(&meta.global_params).unwrap_or_else(|_| "{}".into())),
        ..Default::default()
    }
    .insert(txn)
    .await
    .map_err(|e| format!("迁移项目 {project_key} 失败：{e}"))?;

    // 根分组哨兵
    let root_group = group::ActiveModel {
        project_id: Set(p.id),
        parent_id: Set(None),
        key: Set(String::new()),
        name: Set(String::new()),
        description: Set(String::new()),
        ..Default::default()
    }
    .insert(txn)
    .await
    .map_err(|e| format!("迁移项目 {project_key} 根分组失败：{e}"))?;

    // 环境
    let envs_dir = project_dir.join("environments");
    if envs_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&envs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || path.extension().is_none_or(|e| e != "json") {
                    continue;
                }
                let file_key = path.file_stem().unwrap().to_string_lossy().into_owned();
                let Some(env) = read_json::<EnvironmentFile>(&path) else { continue };
                repo::insert_environment(
                    txn,
                    p.id,
                    &EnvironmentFile { file: file_key, ..env },
                )
                .await?;
            }
        }
    }

    // 接口树
    import_tree_dir(txn, p.id, root_group.id, &project_dir.join("api")).await?;
    Ok(())
}

/// 递归导入分组/接口（与旧 scan_tree_dir 的跳过规则一致）
async fn import_tree_dir(
    txn: &sea_orm::DatabaseTransaction,
    project_id: i32,
    parent_group_id: i32,
    dir: &Path,
) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with('~') {
            continue;
        }
        if path.is_dir() {
            let meta = read_json::<GroupMeta>(&path.join(GROUP_FILE)).unwrap_or(GroupMeta {
                name: name.clone(),
                description: String::new(),
            });
            let g = group::ActiveModel {
                project_id: Set(project_id),
                parent_id: Set(Some(parent_group_id)),
                key: Set(name.clone()),
                name: Set(meta.name),
                description: Set(meta.description),
                ..Default::default()
            }
            .insert(txn)
            .await
            .map_err(|e| format!("迁移分组 {name} 失败：{e}"))?;
            Box::pin(import_tree_dir(txn, project_id, g.id, &path)).await?;
        } else if name != GROUP_FILE && name.ends_with(".json") {
            let key = name.trim_end_matches(".json").to_string();
            let mut iface: InterfaceFile =
                read_json(&path).unwrap_or_else(|| InterfaceFile::new(&key));
            iface.body.migrate_json_content();
            let doc = serde_json::to_string(&iface).map_err(|e| e.to_string())?;
            iface::ActiveModel {
                project_id: Set(project_id),
                group_id: Set(parent_group_id),
                key: Set(key),
                name: Set(iface.name.clone()),
                method: Set(iface.method.clone()),
                doc: Set(doc),
                ..Default::default()
            }
            .insert(txn)
            .await
            .map_err(|e| format!("迁移接口 {name} 失败：{e}"))?;
        }
    }
    Ok(())
}

/// 把旧文件移动到备份目录（不删除）；失败仅提示不阻断
fn archive_legacy_files(root: &Path) {
    let stamp = chrono_stamp();
    let backup = root.join(format!(".file-storage-backup-{stamp}"));
    let mut moved = false;
    for name in [MODULE_DIR, WORKSPACE_FILE] {
        let src = root.join(name);
        if src.exists() {
            if !moved {
                if fs::create_dir_all(&backup).is_err() {
                    eprintln!("旧文件归档失败：无法创建备份目录 {}", backup.display());
                    return;
                }
                moved = true;
            }
            if let Err(e) = fs::rename(&src, backup.join(name)) {
                eprintln!("旧文件归档失败（{}）：{e}", src.display());
            }
        }
    }
    if moved {
        eprintln!("旧版文件数据已归档至 {}", backup.display());
    }
}

fn chrono_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 仅用时间戳避免命名冲突，格式：<unix秒>
    now.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn legacy_file_tree_imports_into_db() {
        let root = std::env::temp_dir().join(format!(
            "apidock-legacy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let api = root.join("api-mgmt");
        let team_dir = api.join("ops");
        let proj_dir = team_dir.join("user-api");
        let group_dir = proj_dir.join("api").join("auth");
        let env_dir = proj_dir.join("environments");
        std::fs::create_dir_all(&group_dir).unwrap();
        std::fs::create_dir_all(&env_dir).unwrap();

        // workspace.json（带注释，验证 JSONC 宽容解析）
        std::fs::write(
            root.join("workspace.json"),
            r#"{
  // 标签栏状态
  "version": 1,
  "openTabs": [ { "teamKey": "ops", "projectKey": "user-api" } ],
  "activeTab": "project:ops:user-api",
  "proxy": { "enabled": false, "kind": "system", "url": "" },
}"#,
        )
        .unwrap();
        std::fs::write(
            team_dir.join("team.json"),
            r#"{"version":1,"name":"运维团队","description":"基础设施"}"#,
        )
        .unwrap();
        std::fs::write(
            proj_dir.join("project.json"),
            r#"{"version":1,"name":"用户中心","description":"","activeEnvironmentId":"env-test","globalVariables":[{"key":"host","value":"http://x","enabled":true}],"globalParams":{"headers":[],"cookies":[],"query":[]}}"#,
        )
        .unwrap();
        std::fs::write(
            env_dir.join("test.json"),
            r#"{"version":1,"id":"env-test","file":"test","name":"测试环境","host":"https://t.example.com","builtin":true,"variables":[]}"#,
        )
        .unwrap();
        std::fs::write(
            group_dir.join("group.json"),
            r#"{"version":1,"name":"鉴权","description":""}"#,
        )
        .unwrap();
        std::fs::write(
            group_dir.join("login.json"),
            r#"{
  "version": 1,
  "id": "abc",
  "name": "登录",
  "method": "POST",
  "url": "/login",
  "headers": [], "query": [],
  "body": { "mode": "none", "content": "", "contentType": "", "json": { "root": { "key":"","name":"","required":false,"type":"object","example":"","description":"","children":[],"items":null } }, "form": [], "filePath": null },
  "auth": { "kind":"none","token":"","username":"","password":"","apiKeyName":"","apiKeyIn":"","apiKeyValue":"" },
  "variables": [], "assertions": [], "description": "",
}"#,
        )
        .unwrap();

        // 打开根目录 → 触发一次性迁移
        let db = crate::db::open(&root).await.unwrap();

        // 团队 / 项目 / 树 / 接口 / 环境 / 工作区均已入库
        let teams = crate::db::repo::list_teams(&db).await;
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].name, "运维团队");
        assert_eq!(crate::db::repo::list_projects(&db, "ops").await[0].name, "用户中心");

        let tree = crate::db::repo::list_interface_tree(&db, "ops", "user-api").await;
        assert_eq!(tree.len(), 1);
        match &tree[0] {
            crate::domain::TreeNode::Group { key, name, children } => {
                assert_eq!(key, "auth");
                assert_eq!(name, "鉴权");
                assert_eq!(children.len(), 1);
                assert!(matches!(&children[0], crate::domain::TreeNode::Interface { key, method, .. } if key == "login" && method == "POST"));
            }
            _ => panic!("expected auth group"),
        }
        let iface = crate::db::repo::get_interface(&db, "ops", "user-api", &["auth".to_string()], "login").await.unwrap();
        assert_eq!(iface.name, "登录");
        assert_eq!(iface.url, "/login");

        let envs = crate::db::repo::list_environments(&db, "ops", "user-api").await;
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].host, "https://t.example.com");
        assert!(envs[0].active); // project.json 中 activeEnvironmentId = env-test

        let settings = crate::db::repo::get_project_settings(&db, "ops", "user-api").await.unwrap();
        assert_eq!(settings.global_variables[0].key, "host");

        let ws = crate::db::repo::get_workspace(&db).await;
        assert_eq!(ws.open_tabs.len(), 1);
        assert_eq!(ws.active_tab.as_deref(), Some("project:ops:user-api"));

        // 旧文件已归档，根目录不再保留原文件树
        assert!(!api.exists());
        assert!(!root.join("workspace.json").exists());
        let backups: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".file-storage-backup-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].path().join("api-mgmt").exists());

        // 再次打开不重复迁移、数据不变
        drop(db);
        let db2 = crate::db::open(&root).await.unwrap();
        assert_eq!(crate::db::repo::list_teams(&db2).await.len(), 1);
    }
}
