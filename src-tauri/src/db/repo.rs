//! 仓储层：全部数据读写语义（对齐原 storage.rs 的行为与错误文案）

use super::entity::{environment, group, iface, project, team, workspace};
use crate::domain::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, Set, TransactionTrait,
};
use std::collections::HashMap;

fn db_err(e: sea_orm::DbErr) -> String {
    format!("数据库错误：{e}")
}

fn parse_json<T: serde::de::DeserializeOwned>(s: &str, fallback: T) -> T {
    serde_json::from_str(s).unwrap_or(fallback)
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| format!("序列化失败：{e}"))
}

// ----- 内部解析 -----

async fn find_team_opt(
    db: &DatabaseConnection,
    key: &str,
) -> Result<Option<team::Model>, String> {
    team::Entity::find()
        .filter(team::Column::Key.eq(key))
        .one(db)
        .await
        .map_err(db_err)
}

async fn find_team(db: &DatabaseConnection, key: &str) -> Result<team::Model, String> {
    find_team_opt(db, key)
        .await?
        .ok_or_else(|| format!("团队 {key} 不存在"))
}

async fn find_project_opt(
    db: &DatabaseConnection,
    team_id: i32,
    key: &str,
) -> Result<Option<project::Model>, String> {
    project::Entity::find()
        .filter(project::Column::TeamId.eq(team_id))
        .filter(project::Column::Key.eq(key))
        .one(db)
        .await
        .map_err(db_err)
}

async fn find_project(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Result<(team::Model, project::Model), String> {
    let t = find_team(db, team_key).await?;
    let p = find_project_opt(db, t.id, project_key)
        .await?
        .ok_or_else(|| format!("项目 {project_key} 不存在"))?;
    Ok((t, p))
}

async fn root_group(
    db: &(impl sea_orm::ConnectionTrait + TransactionTrait),
    project_id: i32,
) -> Result<group::Model, String> {
    group::Entity::find()
        .filter(group::Column::ProjectId.eq(project_id))
        .filter(group::Column::ParentId.is_null())
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| "项目根分组缺失".to_string())
}

/// 按分组路径解析分组（空路径 = 项目根分组）
async fn resolve_group(
    db: &DatabaseConnection,
    project_id: i32,
    path: &[String],
) -> Result<group::Model, String> {
    let mut cur = root_group(db, project_id).await?;
    for seg in path {
        cur = group::Entity::find()
            .filter(group::Column::ParentId.eq(cur.id))
            .filter(group::Column::Key.eq(seg.as_str()))
            .one(db)
            .await
            .map_err(db_err)?
            .ok_or_else(|| format!("分组 {seg} 不存在"))?;
    }
    Ok(cur)
}

/// 解析路径，缺失的中间分组自动创建（名称 = 键）。用于移动操作。
async fn ensure_group_path(
    txn: &DatabaseTransaction,
    project_id: i32,
    path: &[String],
) -> Result<group::Model, String> {
    let mut cur = root_group(txn, project_id).await?;
    for seg in path {
        let existing = group::Entity::find()
            .filter(group::Column::ParentId.eq(cur.id))
            .filter(group::Column::Key.eq(seg.as_str()))
            .one(txn)
            .await
            .map_err(db_err)?;
        cur = match existing {
            Some(g) => g,
            None => {
                let occupied = iface::Entity::find()
                    .filter(iface::Column::GroupId.eq(cur.id))
                    .filter(iface::Column::Key.eq(seg.as_str()))
                    .one(txn)
                    .await
                    .map_err(db_err)?
                    .is_some();
                if occupied {
                    return Err(format!("分组键 {seg} 已被接口占用"));
                }
                group::ActiveModel {
                    project_id: Set(project_id),
                    parent_id: Set(Some(cur.id)),
                    key: Set(seg.clone()),
                    name: Set(seg.clone()),
                    description: Set(String::new()),
                    ..Default::default()
                }
                .insert(txn)
                .await
                .map_err(db_err)?
            }
        };
    }
    Ok(cur)
}

/// 同级下该键是否已被分组或接口占用
async fn sibling_key_taken(
    db: &(impl sea_orm::ConnectionTrait + TransactionTrait),
    parent_id: i32,
    key: &str,
) -> Result<bool, String> {
    let g = group::Entity::find()
        .filter(group::Column::ParentId.eq(parent_id))
        .filter(group::Column::Key.eq(key))
        .one(db)
        .await
        .map_err(db_err)?
        .is_some();
    if g {
        return Ok(true);
    }
    let i = iface::Entity::find()
        .filter(iface::Column::GroupId.eq(parent_id))
        .filter(iface::Column::Key.eq(key))
        .one(db)
        .await
        .map_err(db_err)?
        .is_some();
    Ok(i)
}

async fn find_iface_row(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> Result<(project::Model, group::Model, iface::Model), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let g = resolve_group(db, p.id, group_path).await?;
    let row = iface::Entity::find()
        .filter(iface::Column::GroupId.eq(g.id))
        .filter(iface::Column::Key.eq(iface_key))
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| format!("接口 {iface_key} 不存在"))?;
    Ok((p, g, row))
}

/// 分组子树的全部 id（含自身）与深度（用于按深度倒序删除）
async fn subtree_groups(
    db: &(impl sea_orm::ConnectionTrait + TransactionTrait),
    root_id: i32,
) -> Result<Vec<(i32, usize)>, String> {
    let mut out = vec![(root_id, 0usize)];
    let mut frontier = vec![root_id];
    let mut depth = 0usize;
    while !frontier.is_empty() {
        depth += 1;
        let children = group::Entity::find()
            .filter(group::Column::ParentId.is_in(frontier.clone()))
            .all(db)
            .await
            .map_err(db_err)?;
        frontier = children.iter().map(|c| c.id).collect();
        for c in &children {
            out.push((c.id, depth));
        }
    }
    Ok(out)
}

// ----- workspace -----

pub async fn get_workspace(db: &DatabaseConnection) -> WorkspaceState {
    let Ok(row) = workspace::Entity::find_by_id(1).one(db).await else {
        return WorkspaceState::new();
    };
    let Some(w) = row else {
        return WorkspaceState::new();
    };
    WorkspaceState {
        version: SCHEMA_VERSION,
        open_tabs: parse_json(&w.open_tabs, Vec::new()),
        active_tab: w.active_tab.clone(),
        proxy: parse_json(&w.proxy, ProxyConfig::default()),
    }
}

pub async fn save_workspace(db: &DatabaseConnection, state: &WorkspaceState) -> Result<(), String> {
    let open_tabs = to_json(&state.open_tabs)?;
    let proxy = to_json(&state.proxy)?;
    workspace::Entity::insert(workspace::ActiveModel {
        id: Set(1),
        open_tabs: Set(open_tabs),
        active_tab: Set(state.active_tab.clone()),
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
    .exec(db)
    .await
    .map_err(db_err)?;
    Ok(())
}

// ----- 团队 -----

pub async fn list_teams(db: &DatabaseConnection) -> Vec<TeamInfo> {
    let Ok(rows) = team::Entity::find().all(db).await else {
        return Vec::new();
    };
    let mut teams: Vec<TeamInfo> = rows
        .into_iter()
        .map(|t| TeamInfo { key: t.key, name: t.name })
        .collect();
    teams.sort_by(|a, b| a.name.cmp(&b.name));
    teams
}

pub async fn create_team(
    db: &DatabaseConnection,
    key: &str,
    name: &str,
) -> Result<TeamInfo, String> {
    if find_team_opt(db, key).await?.is_some() {
        return Err(format!("团队键 {key} 已存在"));
    }
    team::ActiveModel {
        key: Set(key.to_string()),
        name: Set(name.to_string()),
        description: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)?;
    Ok(TeamInfo { key: key.to_string(), name: name.to_string() })
}

pub async fn set_team_description(
    db: &DatabaseConnection,
    team_key: &str,
    description: &str,
) -> Result<(), String> {
    let t = find_team(db, team_key).await?;
    let mut am: team::ActiveModel = t.into();
    am.description = Set(description.to_string());
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn rename_team(
    db: &DatabaseConnection,
    team_key: &str,
    new_name: &str,
) -> Result<(), String> {
    let t = find_team(db, team_key).await?;
    if new_name != t.key && find_team_opt(db, new_name).await?.is_some() {
        return Err(format!("键 {new_name} 已存在"));
    }
    let mut am: team::ActiveModel = t.into();
    am.key = Set(new_name.to_string());
    am.name = Set(new_name.to_string());
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn delete_team(db: &DatabaseConnection, team_key: &str) -> Result<(), String> {
    let t = find_team(db, team_key).await?;
    let txn = db.begin().await.map_err(db_err)?;
    let projects = project::Entity::find()
        .filter(project::Column::TeamId.eq(t.id))
        .all(&txn)
        .await
        .map_err(db_err)?;
    for p in &projects {
        delete_project_rows(&txn, p.id).await?;
    }
    team::Entity::delete_by_id(t.id)
        .exec(&txn)
        .await
        .map_err(db_err)?;
    txn.commit().await.map_err(db_err)?;
    Ok(())
}

// ----- 项目 -----

pub async fn list_projects(db: &DatabaseConnection, team_key: &str) -> Vec<ProjectInfo> {
    let Ok(t) = find_team_opt(db, team_key).await else {
        return Vec::new();
    };
    let Some(t) = t else {
        return Vec::new();
    };
    let Ok(rows) = project::Entity::find()
        .filter(project::Column::TeamId.eq(t.id))
        .all(db)
        .await
    else {
        return Vec::new();
    };
    let mut projects: Vec<ProjectInfo> = rows
        .into_iter()
        .map(|p| ProjectInfo { key: p.key, name: p.name })
        .collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
}

/// 项目默认三套内置环境
pub(crate) fn default_environments() -> Vec<(String, EnvironmentFile)> {
    vec![
        (
            "prod".to_string(),
            EnvironmentFile { version: SCHEMA_VERSION, id: "env-prod".into(), file: "prod".into(), name: "正式环境".into(), host: String::new(), builtin: true, variables: Vec::new() },
        ),
        (
            "test".to_string(),
            EnvironmentFile { version: SCHEMA_VERSION, id: "env-test".into(), file: "test".into(), name: "测试环境".into(), host: String::new(), builtin: true, variables: Vec::new() },
        ),
        (
            "dev".to_string(),
            EnvironmentFile { version: SCHEMA_VERSION, id: "env-dev".into(), file: "dev".into(), name: "开发环境".into(), host: String::new(), builtin: true, variables: Vec::new() },
        ),
    ]
}

pub(crate) async fn insert_environment(
    txn: &DatabaseTransaction,
    project_id: i32,
    env: &EnvironmentFile,
) -> Result<(), String> {
    environment::ActiveModel {
        project_id: Set(project_id),
        env_id: Set(env.id.clone()),
        file_key: Set(env.file.clone()),
        name: Set(env.name.clone()),
        host: Set(env.host.clone()),
        builtin: Set(env.builtin),
        variables: Set(to_json(&env.variables)?),
        ..Default::default()
    }
    .insert(txn)
    .await
    .map_err(db_err)?;
    Ok(())
}

pub async fn create_project(
    db: &DatabaseConnection,
    team_key: &str,
    key: &str,
    name: &str,
) -> Result<ProjectInfo, String> {
    let t = find_team(db, team_key).await?;
    if find_project_opt(db, t.id, key).await?.is_some() {
        return Err(format!("项目键 {key} 已存在"));
    }
    let txn = db.begin().await.map_err(db_err)?;
    let p = project::ActiveModel {
        team_id: Set(t.id),
        key: Set(key.to_string()),
        name: Set(name.to_string()),
        description: Set(String::new()),
        active_environment_id: Set(Some("env-prod".into())),
        global_variables: Set("[]".into()),
        global_params: Set("{}".into()),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(db_err)?;
    // 根分组哨兵行
    group::ActiveModel {
        project_id: Set(p.id),
        parent_id: Set(None),
        key: Set(String::new()),
        name: Set(String::new()),
        description: Set(String::new()),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(db_err)?;
    // 默认三套环境
    for (_, env) in default_environments() {
        insert_environment(&txn, p.id, &env).await?;
    }
    txn.commit().await.map_err(db_err)?;
    Ok(ProjectInfo { key: key.to_string(), name: name.to_string() })
}

/// 删除项目下全部行（事务内调用）：接口 → 分组 → 环境 → 项目
async fn delete_project_rows(txn: &DatabaseTransaction, project_id: i32) -> Result<(), String> {
    iface::Entity::delete_many()
        .filter(iface::Column::ProjectId.eq(project_id))
        .exec(txn)
        .await
        .map_err(db_err)?;
    // 分组按深度倒序删，避免外键父先于子被删
    let all = group::Entity::find()
        .filter(group::Column::ProjectId.eq(project_id))
        .all(txn)
        .await
        .map_err(db_err)?;
    let by_id: HashMap<i32, Option<i32>> = all.iter().map(|g| (g.id, g.parent_id)).collect();
    let depth = |mut id: i32| {
        let mut d = 0usize;
        while let Some(Some(parent)) = by_id.get(&id) {
            d += 1;
            id = *parent;
        }
        d
    };
    let mut rows: Vec<(i32, usize)> = all.iter().map(|g| (g.id, depth(g.id))).collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (id, _) in rows {
        group::Entity::delete_by_id(id)
            .exec(txn)
            .await
            .map_err(db_err)?;
    }
    environment::Entity::delete_many()
        .filter(environment::Column::ProjectId.eq(project_id))
        .exec(txn)
        .await
        .map_err(db_err)?;
    project::Entity::delete_many()
        .filter(project::Column::Id.eq(project_id))
        .exec(txn)
        .await
        .map_err(db_err)?;
    Ok(())
}

pub async fn delete_project(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let txn = db.begin().await.map_err(db_err)?;
    delete_project_rows(&txn, p.id).await?;
    txn.commit().await.map_err(db_err)?;
    Ok(())
}

pub async fn rename_project(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    new_name: &str,
) -> Result<(), String> {
    let (t, p) = find_project(db, team_key, project_key).await?;
    if new_name != p.key && find_project_opt(db, t.id, new_name).await?.is_some() {
        return Err(format!("键 {new_name} 已存在"));
    }
    let mut am: project::ActiveModel = p.into();
    am.key = Set(new_name.to_string());
    am.name = Set(new_name.to_string());
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn set_project_description(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    description: &str,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let mut am: project::ActiveModel = p.into();
    am.description = Set(description.to_string());
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn get_project_settings(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Result<ProjectSettings, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    Ok(ProjectSettings {
        name: p.name.clone(),
        active_environment_id: p.active_environment_id.clone(),
        global_variables: parse_json(&p.global_variables, Vec::new()),
        global_params: parse_json(&p.global_params, GlobalParams::default()),
    })
}

pub async fn save_project_settings(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    settings: ProjectSettings,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let mut am: project::ActiveModel = p.into();
    am.name = Set(settings.name);
    am.global_variables = Set(to_json(&settings.global_variables)?);
    am.global_params = Set(to_json(&settings.global_params)?);
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

// ----- 接口树 -----

pub async fn list_interface_tree(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Vec<TreeNode> {
    let Ok(t) = find_team_opt(db, team_key).await else {
        return Vec::new();
    };
    let Some(t) = t else {
        return Vec::new();
    };
    let Ok(Some(p)) = find_project_opt(db, t.id, project_key).await else {
        return Vec::new();
    };
    let Ok(groups) = group::Entity::find()
        .filter(group::Column::ProjectId.eq(p.id))
        .all(db)
        .await
    else {
        return Vec::new();
    };
    let Ok(ifaces) = iface::Entity::find()
        .filter(iface::Column::ProjectId.eq(p.id)).all(db).await
    else {
        return Vec::new();
    };

    let root_id = groups.iter().find(|g| g.parent_id.is_none()).map(|g| g.id);

    // 桶：父分组 id -> (排序键, 分组 id 或 None, 节点)
    let mut buckets: HashMap<Option<i32>, Vec<(String, Option<i32>, TreeNode)>> = HashMap::new();
    for g in &groups {
        if g.parent_id.is_none() {
            continue; // 根分组哨兵不出现在树中
        }
        let node = TreeNode::Group { key: g.key.clone(), name: g.name.clone(), children: Vec::new() };
        buckets
            .entry(g.parent_id)
            .or_default()
            .push((sort_key(&g.key, true), Some(g.id), node));
    }
    for i in &ifaces {
        let node = TreeNode::Interface { key: i.key.clone(), name: i.name.clone(), method: i.method.clone() };
        buckets
            .entry(Some(i.group_id))
            .or_default()
            .push((sort_key(&i.key, false), None, node));
    }

    build_level(root_id.map(Some).unwrap_or(None), &mut buckets)
}

/// 与旧文件系统一致的排序键：分组为键本身，接口为「键.json」
fn sort_key(key: &str, is_group: bool) -> String {
    if is_group {
        key.to_string()
    } else {
        format!("{key}.json")
    }
}

fn build_level(
    parent: Option<i32>,
    buckets: &mut HashMap<Option<i32>, Vec<(String, Option<i32>, TreeNode)>>,
) -> Vec<TreeNode> {
    let mut list = buckets.remove(&parent).unwrap_or_default();
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list.into_iter()
        .map(|(_, gid, node)| match node {
            TreeNode::Group { key, name, .. } => TreeNode::Group {
                key,
                name,
                children: gid.map(|id| build_level(Some(id), buckets)).unwrap_or_default(),
            },
            other => other,
        })
        .collect()
}

pub async fn create_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    key: &str,
    name: &str,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let parent = resolve_group(db, p.id, group_path).await?;
    if sibling_key_taken(db, parent.id, key).await? {
        return Err(format!("分组键 {key} 已存在"));
    }
    group::ActiveModel {
        project_id: Set(p.id),
        parent_id: Set(Some(parent.id)),
        key: Set(key.to_string()),
        name: Set(name.to_string()),
        description: Set(String::new()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)?;
    Ok(())
}

pub async fn set_group_description(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    description: &str,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let g = resolve_group(db, p.id, group_path).await?;
    let mut am: group::ActiveModel = g.into();
    am.description = Set(description.to_string());
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn rename_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    new_name: &str,
) -> Result<(), String> {
    if group_path.is_empty() {
        return Err("不能重命名项目根".into());
    }
    let (_, p) = find_project(db, team_key, project_key).await?;
    let g = resolve_group(db, p.id, group_path).await?;
    let old_key = group_path.last().map(String::as_str).unwrap_or_default();
    let parent_id = g.parent_id.ok_or("不能重命名项目根")?;
    if new_name != old_key && sibling_key_taken(db, parent_id, new_name).await? {
        return Err(format!("已存在同名分组 {new_name}"));
    }
    let mut am: group::ActiveModel = g.into();
    am.key = Set(new_name.to_string());
    am.name = Set(new_name.to_string());
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn delete_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let g = resolve_group(db, p.id, group_path).await?;
    if g.parent_id.is_none() {
        return Err("不能删除项目根".into());
    }
    let txn = db.begin().await.map_err(db_err)?;
    let mut subtree = subtree_groups(&txn, g.id).await?;
    let ids: Vec<i32> = subtree.iter().map(|(id, _)| *id).collect();
    iface::Entity::delete_many()
        .filter(iface::Column::GroupId.is_in(ids.clone()))
        .exec(&txn)
        .await
        .map_err(db_err)?;
    subtree.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (id, _) in subtree {
        group::Entity::delete_by_id(id)
            .exec(&txn)
            .await
            .map_err(db_err)?;
    }
    txn.commit().await.map_err(db_err)?;
    Ok(())
}

/// 移动分组到目标分组下（目标路径缺失时自动创建），返回 Err 若目标在自身子树内等
pub async fn move_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    target_group_path: &[String],
) -> Result<(), String> {
    if group_path.is_empty() {
        return Err("不能移动根级".into());
    }
    if group_path == target_group_path {
        return Err("目标即自身".into());
    }
    if group_path.len() <= target_group_path.len()
        && group_path[..] == target_group_path[..group_path.len()]
    {
        return Err("不能移动到自己的子分组下".into());
    }
    let (_, p) = find_project(db, team_key, project_key).await?;
    let g = resolve_group(db, p.id, group_path).await?;
    let key = group_path.last().cloned().unwrap_or_default();

    let txn = db.begin().await.map_err(db_err)?;
    let target_parent = ensure_group_path(&txn, p.id, target_group_path).await?;
    if sibling_key_taken(&txn, target_parent.id, &key).await? {
        return Err(format!("目标分组下已存在 {key}"));
    }
    let mut am: group::ActiveModel = g.into();
    am.parent_id = Set(Some(target_parent.id));
    am.update(&txn).await.map_err(db_err)?;
    txn.commit().await.map_err(db_err)?;
    Ok(())
}

// ----- 接口 -----

pub async fn create_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    key: &str,
    name: &str,
) -> Result<InterfaceFile, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let parent = resolve_group(db, p.id, group_path).await?;
    if sibling_key_taken(db, parent.id, key).await? {
        return Err(format!("接口键 {key} 已存在"));
    }
    let mut iface = InterfaceFile::new(key);
    if !name.trim().is_empty() {
        iface.name = name.to_string();
    }
    iface::ActiveModel {
        project_id: Set(p.id),
        group_id: Set(parent.id),
        key: Set(key.to_string()),
        name: Set(iface.name.clone()),
        method: Set(iface.method.clone()),
        doc: Set(to_json(&iface)?),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)?;
    Ok(iface)
}

pub async fn get_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> Result<InterfaceFile, String> {
    let (_, _, row) = find_iface_row(db, team_key, project_key, group_path, iface_key).await?;
    let iface: InterfaceFile = parse_json(&row.doc, InterfaceFile::new(&row.key));
    Ok(iface)
}

/// 保存整个接口定义；json 模式且结构树非空时重新生成 content（示例载荷）
pub async fn save_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    iface: &InterfaceFile,
) -> Result<(), String> {
    let (_, _, row) = find_iface_row(db, team_key, project_key, group_path, iface_key).await?;
    let mut doc = iface.clone();
    if let Some(payload) = doc.body.json_example_payload() {
        doc.body.content = payload;
    }
    let mut am: iface::ActiveModel = row.into();
    am.name = Set(doc.name.clone());
    am.method = Set(doc.method.clone());
    am.doc = Set(to_json(&doc)?);
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn rename_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    new_name: &str,
) -> Result<(), String> {
    let new_name = validate_name(new_name)?;
    let (_, g, row) = find_iface_row(db, team_key, project_key, group_path, iface_key).await?;
    if new_name != iface_key && sibling_key_taken(db, g.id, &new_name).await? {
        return Err(format!("已存在同名接口 {new_name}"));
    }
    let mut doc: InterfaceFile = parse_json(&row.doc, InterfaceFile::new(&new_name));
    doc.name = new_name.clone();
    let mut am: iface::ActiveModel = row.into();
    am.key = Set(new_name);
    am.name = Set(doc.name.clone());
    am.doc = Set(to_json(&doc)?);
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

pub async fn delete_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> Result<(), String> {
    let (_, _, row) = find_iface_row(db, team_key, project_key, group_path, iface_key).await?;
    iface::Entity::delete_by_id(row.id)
        .exec(db)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// 移动接口到目标分组（目标分组缺失时自动创建）。返回接口键。
pub async fn move_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    target_group_path: &[String],
) -> Result<String, String> {
    let (_, _, row) = find_iface_row(db, team_key, project_key, group_path, iface_key).await?;
    let txn = db.begin().await.map_err(db_err)?;
    let target = ensure_group_path(&txn, row.project_id, target_group_path).await?;
    let conflict = iface::Entity::find()
        .filter(iface::Column::GroupId.eq(target.id))
        .filter(iface::Column::Key.eq(iface_key))
        .one(&txn)
        .await
        .map_err(db_err)?
        .is_some();
    if conflict {
        return Err(format!("目标分组已存在接口键 {iface_key}"));
    }
    let mut am: iface::ActiveModel = row.into();
    am.group_id = Set(target.id);
    am.update(&txn).await.map_err(db_err)?;
    txn.commit().await.map_err(db_err)?;
    Ok(iface_key.to_string())
}

// ----- 环境 -----

pub async fn list_environments(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Vec<EnvironmentSummary> {
    let Ok(t) = find_team_opt(db, team_key).await else {
        return Vec::new();
    };
    let Some(t) = t else {
        return Vec::new();
    };
    let Ok(Some(p)) = find_project_opt(db, t.id, project_key).await else {
        return Vec::new();
    };
    let active = p.active_environment_id.clone();
    let Ok(rows) = environment::Entity::find()
        .filter(environment::Column::ProjectId.eq(p.id))
        .all(db)
        .await
    else {
        return Vec::new();
    };
    let mut list: Vec<EnvironmentSummary> = rows
        .into_iter()
        .map(|e| EnvironmentSummary {
            active: Some(e.env_id.clone()) == active,
            id: e.env_id,
            file: e.file_key,
            name: e.name,
            host: e.host,
            builtin: e.builtin,
        })
        .collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

pub async fn get_environment(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    env_id: &str,
) -> Result<EnvironmentFile, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let row = environment::Entity::find()
        .filter(environment::Column::ProjectId.eq(p.id))
        .filter(environment::Column::EnvId.eq(env_id))
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| format!("环境 {env_id} 不存在"))?;
    Ok(EnvironmentFile {
        version: SCHEMA_VERSION,
        id: row.env_id,
        file: row.file_key,
        name: row.name,
        host: row.host,
        builtin: row.builtin,
        variables: parse_json(&row.variables, Vec::new()),
    })
}

pub async fn save_environment(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    env: EnvironmentFile,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    // 文件键冲突检查（被其它环境占用时拒绝）
    let file_taken = environment::Entity::find()
        .filter(environment::Column::ProjectId.eq(p.id))
        .filter(environment::Column::FileKey.eq(env.file.as_str()))
        .filter(environment::Column::EnvId.ne(env.id.as_str()))
        .one(db)
        .await
        .map_err(db_err)?
        .is_some();
    if file_taken {
        return Err(format!("环境文件键 {} 已被占用", env.file));
    }
    let existing = environment::Entity::find()
        .filter(environment::Column::ProjectId.eq(p.id))
        .filter(environment::Column::EnvId.eq(env.id.as_str()))
        .one(db)
        .await
        .map_err(db_err)?;
    let variables = to_json(&env.variables)?;
    match existing {
        Some(row) => {
            let mut am: environment::ActiveModel = row.into();
            am.file_key = Set(env.file);
            am.name = Set(env.name);
            am.host = Set(env.host);
            am.builtin = Set(env.builtin);
            am.variables = Set(variables);
            am.update(db).await.map_err(db_err)?;
        }
        None => {
            environment::ActiveModel {
                project_id: Set(p.id),
                env_id: Set(env.id),
                file_key: Set(env.file),
                name: Set(env.name),
                host: Set(env.host),
                builtin: Set(env.builtin),
                variables: Set(variables),
                ..Default::default()
            }
            .insert(db)
            .await
            .map_err(db_err)?;
        }
    }
    Ok(())
}

pub async fn delete_environment(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    env_id: &str,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let row = environment::Entity::find()
        .filter(environment::Column::ProjectId.eq(p.id))
        .filter(environment::Column::EnvId.eq(env_id))
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| format!("环境 {env_id} 不存在"))?;
    if row.builtin {
        return Err("内置环境不可删除".into());
    }
    environment::Entity::delete_by_id(row.id)
        .exec(db)
        .await
        .map_err(db_err)?;
    Ok(())
}

pub async fn set_active_environment(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    env_id: &str,
) -> Result<(), String> {
    get_environment(db, team_key, project_key, env_id).await?;
    let (_, p) = find_project(db, team_key, project_key).await?;
    let mut am: project::ActiveModel = p.into();
    am.active_environment_id = Set(Some(env_id.to_string()));
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup(tag: &str) -> DatabaseConnection {
        let db = crate::db::tests_support::temp_db(tag).await;
        create_team(&db, "ops", "运维团队").await.unwrap();
        create_project(&db, "ops", "user-api", "用户中心").await.unwrap();
        db
    }

    #[tokio::test]
    async fn team_and_project_crud() {
        let db = crate::db::tests_support::temp_db("crud").await;

        let team = create_team(&db, "ops", "运维团队").await.unwrap();
        assert_eq!(team.name, "运维团队");
        assert!(create_team(&db, "ops", "重复").await.is_err());

        let proj = create_project(&db, "ops", "user-api", "用户中心").await.unwrap();
        assert_eq!(proj.name, "用户中心");
        assert!(create_project(&db, "ops", "user-api", "x").await.is_err());
        assert!(create_project(&db, "missing", "x", "y").await.is_err());

        assert_eq!(list_teams(&db).await.len(), 1);
        assert_eq!(list_projects(&db, "ops").await.len(), 1);

        // 描述与重命名
        set_team_description(&db, "ops", "负责基础设施").await.unwrap();
        rename_team(&db, "ops", "运维研发").await.unwrap();
        assert!(list_projects(&db, "ops").await.is_empty());
        assert_eq!(list_projects(&db, "运维研发").await.len(), 1);
        // 重命名冲突
        create_team(&db, "other", "其他").await.unwrap();
        assert!(rename_team(&db, "other", "运维研发").await.is_err());

        // workspace 标签状态持久化
        let ws = WorkspaceState {
            version: 1,
            open_tabs: vec![OpenTab { team_key: "运维研发".into(), project_key: "user-api".into() }],
            active_tab: Some("project:运维研发:user-api".into()),
            proxy: ProxyConfig::default(),
        };
        save_workspace(&db, &ws).await.unwrap();
        assert_eq!(get_workspace(&db).await.open_tabs.len(), 1);
        // 二次保存为覆盖而非新增
        save_workspace(&db, &ws).await.unwrap();
        assert_eq!(get_workspace(&db).await.open_tabs.len(), 1);

        delete_project(&db, "运维研发", "user-api").await.unwrap();
        delete_team(&db, "运维研发").await.unwrap();
        delete_team(&db, "other").await.unwrap();
        assert!(list_teams(&db).await.is_empty());
    }

    #[tokio::test]
    async fn project_creates_default_environments() {
        let db = setup("envs").await;
        let envs = list_environments(&db, "ops", "user-api").await;
        assert_eq!(envs.len(), 3);
        assert!(envs.iter().any(|e| e.id == "env-prod" && e.active));
        assert!(envs.iter().all(|e| e.builtin));
    }

    #[tokio::test]
    async fn group_and_interface_tree_crud() {
        let db = setup("tree").await;

        // 建多级分组
        create_group(&db, "ops", "user-api", &[], "auth", "鉴权").await.unwrap();
        create_group(&db, "ops", "user-api", &["auth".to_string()], "login", "登录").await.unwrap();
        assert!(create_group(&db, "ops", "user-api", &[], "auth", "x").await.is_err());

        // 根级与分组级接口
        let iface = create_interface(&db, "ops", "user-api", &[], "health", "健康检查").await.unwrap();
        assert_eq!(iface.method, "GET");
        assert_eq!(iface.name, "健康检查");
        create_interface(&db, "ops", "user-api", &["auth".to_string(), "login".to_string()], "do-login", "登录接口").await.unwrap();
        assert!(create_interface(&db, "ops", "user-api", &[], "health", "x").await.is_err());

        // 树（按键字母序：auth 分组在前，health 接口在后）
        let tree = list_interface_tree(&db, "ops", "user-api").await;
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

        // 编辑 / 保存
        let mut got = get_interface(&db, "ops", "user-api", &[], "health").await.unwrap();
        got.name = "健康检查改".into();
        got.method = "POST".into();
        got.url = "/ping".into();
        got.description = "说明".into();
        save_interface(&db, "ops", "user-api", &[], "health", &got).await.unwrap();
        let got2 = get_interface(&db, "ops", "user-api", &[], "health").await.unwrap();
        assert_eq!(got2.name, "健康检查改");
        assert_eq!(got2.method, "POST");
        assert_eq!(got2.url, "/ping");
        // 保存后树中的 name/method 冗余列同步更新
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert!(matches!(&tree[1], TreeNode::Interface { name, method, .. } if name == "健康检查改" && method == "POST"));

        // 重命名（键与名称一起更新）
        rename_interface(&db, "ops", "user-api", &[], "health", "健康检查v2").await.unwrap();
        assert_eq!(get_interface(&db, "ops", "user-api", &[], "健康检查v2").await.unwrap().name, "健康检查v2");
        assert!(get_interface(&db, "ops", "user-api", &[], "health").await.is_err());
        // 重命名冲突
        create_interface(&db, "ops", "user-api", &[], "health", "健康检查").await.unwrap();
        assert!(rename_interface(&db, "ops", "user-api", &[], "health", "健康检查v2").await.is_err());
        assert!(rename_interface(&db, "ops", "user-api", &[], "health", "脏/名").await.is_err());

        // 重命名分组（按键排序时 ASCII 接口在前，按存在性校验）
        rename_group(&db, "ops", "user-api", &["auth".to_string()], "鉴权v2").await.unwrap();
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert!(tree.iter().any(|n| matches!(n, TreeNode::Group { key, .. } if key == "鉴权v2")));
        assert!(!tree.iter().any(|n| matches!(n, TreeNode::Group { key, .. } if key == "auth")));

        // 删除
        delete_interface(&db, "ops", "user-api", &[], "健康检查v2").await.unwrap();
        delete_interface(&db, "ops", "user-api", &[], "health").await.unwrap();
        delete_group(&db, "ops", "user-api", &["鉴权v2".to_string()]).await.unwrap();
        assert!(list_interface_tree(&db, "ops", "user-api").await.is_empty());
    }

    #[tokio::test]
    async fn environment_lifecycle() {
        let db = setup("envlife").await;

        let list = list_environments(&db, "ops", "user-api").await;
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|e| e.id == "env-prod" && e.active));

        // 激活其它环境
        set_active_environment(&db, "ops", "user-api", "env-dev").await.unwrap();
        let list = list_environments(&db, "ops", "user-api").await;
        assert!(list.iter().any(|e| e.id == "env-dev" && e.active));

        // 编辑环境（host + 变量）
        let mut dev = get_environment(&db, "ops", "user-api", "env-dev").await.unwrap();
        dev.host = "https://dev.example.com".into();
        dev.variables.push(KeyValue { key: "token".into(), value: "abc".into(), enabled: true });
        save_environment(&db, "ops", "user-api", dev).await.unwrap();
        let dev2 = get_environment(&db, "ops", "user-api", "env-dev").await.unwrap();
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
        save_environment(&db, "ops", "user-api", custom).await.unwrap();
        assert_eq!(list_environments(&db, "ops", "user-api").await.len(), 4);

        // 内置不可删，自定义可删
        assert!(delete_environment(&db, "ops", "user-api", "env-prod").await.is_err());
        delete_environment(&db, "ops", "user-api", "env-staging").await.unwrap();
        assert_eq!(list_environments(&db, "ops", "user-api").await.len(), 3);

        // 项目设置（全局变量/参数）
        let mut settings = get_project_settings(&db, "ops", "user-api").await.unwrap();
        assert_eq!(settings.active_environment_id.as_deref(), Some("env-dev"));
        settings.global_variables.push(KeyValue { key: "host".into(), value: "http://glob.example.com".into(), enabled: true });
        settings.global_params.headers.push(KeyValue { key: "X-Trace".into(), value: "1".into(), enabled: true });
        save_project_settings(&db, "ops", "user-api", settings).await.unwrap();
        let s2 = get_project_settings(&db, "ops", "user-api").await.unwrap();
        assert_eq!(s2.global_variables[0].key, "host");
        assert_eq!(s2.global_params.headers[0].key, "X-Trace");
    }

    #[tokio::test]
    async fn rename_and_move_nodes() {
        let db = setup("move").await;
        create_interface(&db, "ops", "user-api", &[], "a", "A接口").await.unwrap();
        create_group(&db, "ops", "user-api", &[], "g1", "分组1").await.unwrap();
        create_group(&db, "ops", "user-api", &["g1".to_string()], "g1-1", "子分组").await.unwrap();
        create_interface(&db, "ops", "user-api", &["g1".to_string()], "in-g1", "分组内接口").await.unwrap();

        // 重命名团队 / 项目
        rename_team(&db, "ops", "运维研发").await.unwrap();
        rename_project(&db, "运维研发", "user-api", "用户中心").await.unwrap();
        assert_eq!(list_projects(&db, "运维研发").await[0].key, "用户中心");

        // 移动接口到分组
        let new_key = move_interface(&db, "运维研发", "用户中心", &[], "a", &["g1".to_string()]).await.unwrap();
        assert_eq!(new_key, "a");
        assert!(get_interface(&db, "运维研发", "用户中心", &["g1".to_string()], "a").await.is_ok());
        assert!(get_interface(&db, "运维研发", "用户中心", &[], "a").await.is_err());

        // 移动分组 g1 到 ext 下
        create_group(&db, "运维研发", "用户中心", &[], "ext", "外部").await.unwrap();
        move_group(&db, "运维研发", "用户中心", &["g1".to_string()], &["ext".to_string()]).await.unwrap();
        // 子树随行移动
        assert!(get_interface(&db, "运维研发", "用户中心", &["ext".to_string(), "g1".to_string()], "in-g1").await.is_ok());
        assert!(get_interface(&db, "运维研发", "用户中心", &["g1".to_string()], "in-g1").await.is_err());

        // 不能移动到自身子孙
        assert!(move_group(&db, "运维研发", "用户中心", &["ext".to_string()], &["ext".to_string(), "g1".to_string()]).await.is_err());

        // 分组重命名（改键）
        rename_group(&db, "运维研发", "用户中心", &["ext".to_string()], "外部服务").await.unwrap();
        let tree = list_interface_tree(&db, "运维研发", "用户中心").await;
        assert!(matches!(&tree.iter().find(|n| matches!(n, TreeNode::Group { key, .. } if key == "外部服务")), Some(_)));

        // 删除项目级联清空
        delete_project(&db, "运维研发", "用户中心").await.unwrap();
        assert!(list_projects(&db, "运维研发").await.is_empty());
    }
}
