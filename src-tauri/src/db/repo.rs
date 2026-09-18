//! 仓储层：全部数据读写语义（对齐原 storage.rs 的行为与错误文案）

use super::entity::{
    environment, group, iface, project, quick_group, quick_request, request_history, team,
    workspace,
};
use crate::domain::*;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
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
    let Ok(rows) = team::Entity::find().order_by_asc(team::Column::Name).all(db).await else {
        return Vec::new();
    };
    rows.into_iter()
        .map(|t| TeamInfo { key: t.key, name: t.name })
        .collect()
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
    // 外键 CASCADE：项目 → 接口/分组/环境 一并删除
    let t = find_team(db, team_key).await?;
    team::Entity::delete_by_id(t.id)
        .exec(db)
        .await
        .map_err(db_err)?;
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
        .order_by_asc(project::Column::Name)
        .all(db)
        .await
    else {
        return Vec::new();
    };
    rows.into_iter()
        .map(|p| ProjectInfo { key: p.key, name: p.name })
        .collect()
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

/// 删除项目（外键 CASCADE 清理接口/分组/环境，单条语句保证原子）
pub async fn delete_project(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Result<(), String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    project::Entity::delete_by_id(p.id)
        .exec(db)
        .await
        .map_err(db_err)?;
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

    // 桶：父分组 id -> (层级, 手动顺序, 排序键, 分组 id 或 None, 节点)
    // 同层内分组恒在前（层 0，按 sort_order 再按 key），接口在后（层 1，按 sort_order 再按 key）
    let mut buckets: HashMap<Option<i32>, Vec<(u8, i32, String, Option<i32>, TreeNode)>> =
        HashMap::new();
    for g in &groups {
        if g.parent_id.is_none() {
            continue; // 根分组哨兵不出现在树中
        }
        let node = TreeNode::Group { key: g.key.clone(), name: g.name.clone(), children: Vec::new() };
        buckets
            .entry(g.parent_id)
            .or_default()
            .push((0, g.sort_order, sort_key(&g.key, true), Some(g.id), node));
    }
    for i in &ifaces {
        let node = TreeNode::Interface { key: i.key.clone(), name: i.name.clone(), method: i.method.clone() };
        buckets
            .entry(Some(i.group_id))
            .or_default()
            .push((1, i.sort_order, sort_key(&i.key, false), None, node));
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
    buckets: &mut HashMap<Option<i32>, Vec<(u8, i32, String, Option<i32>, TreeNode)>>,
) -> Vec<TreeNode> {
    let mut list = buckets.remove(&parent).unwrap_or_default();
    list.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    list.into_iter()
        .map(|(_, _, _, gid, node)| match node {
            TreeNode::Group { key, name, .. } => TreeNode::Group {
                key,
                name,
                children: gid.map(|id| build_level(Some(id), buckets)).unwrap_or_default(),
            },
            other => other,
        })
        .collect()
}

/// 一次取回项目全部接口定义（含分组路径与键），供导出/批量运行使用，避免逐接口查询
pub async fn list_interfaces_full(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> Vec<(Vec<String>, String, InterfaceFile)> {
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
    let Ok(rows) = iface::Entity::find()
        .filter(iface::Column::ProjectId.eq(p.id))
        .all(db)
        .await
    else {
        return Vec::new();
    };

    // 分组 id -> 完整分组路径（根哨兵路径为空）
    let by_id: HashMap<i32, &group::Model> = groups.iter().map(|g| (g.id, g)).collect();
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut root_id = None;
    for g in &groups {
        match g.parent_id {
            None => root_id = Some(g.id),
            Some(pid) => children.entry(pid).or_default().push(g.id),
        }
    }
    let mut path_of: HashMap<i32, Vec<String>> = HashMap::new();
    if let Some(root) = root_id {
        let mut stack = vec![(root, Vec::new())];
        while let Some((id, path)) = stack.pop() {
            path_of.insert(id, path.clone());
            if let Some(child_ids) = children.get(&id) {
                for cid in child_ids {
                    let mut p = path.clone();
                    if let Some(c) = by_id.get(cid) {
                        p.push(c.key.clone());
                    }
                    stack.push((*cid, p));
                }
            }
        }
    }

    rows.into_iter()
        .map(|r| {
            let path = path_of.get(&r.group_id).cloned().unwrap_or_default();
            let iface = parse_json(&r.doc, InterfaceFile::new(&r.key));
            (path, r.key, iface)
        })
        .collect()
}

/// 同层接口分组的显示顺序（手动顺序优先，未排序时退化为 key 字母序）
async fn group_siblings<C: ConnectionTrait>(
    db: &C,
    parent_id: i32,
) -> Result<Vec<group::Model>, String> {
    let mut rows = group::Entity::find()
        .filter(group::Column::ParentId.eq(parent_id))
        .all(db)
        .await
        .map_err(db_err)?;
    rows.sort_by(|a, b| (a.sort_order, &a.key).cmp(&(b.sort_order, &b.key)));
    Ok(rows)
}

/// 把同层分组按当前显示顺序重排为 1..n（拖拽/移动后写这一列）
async fn renumber_groups<C: ConnectionTrait>(db: &C, ordered: &[i32]) -> Result<(), String> {
    for (idx, id) in ordered.iter().enumerate() {
        group::Entity::update_many()
            .col_expr(group::Column::SortOrder, Expr::value(idx as i32 + 1))
            .filter(group::Column::Id.eq(*id))
            .exec(db)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

/// 补齐旧数据的 0 值顺序（新建分组前调用，保证「新建追加到末尾」成立）
async fn normalize_group_order<C: ConnectionTrait>(db: &C, parent_id: i32) -> Result<(), String> {
    let rows = group_siblings(db, parent_id).await?;
    if rows
        .iter()
        .enumerate()
        .all(|(i, r)| r.sort_order == i as i32 + 1)
    {
        return Ok(());
    }
    let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
    renumber_groups(db, &ids).await
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
    normalize_group_order(db, parent.id).await?;
    let next = group_siblings(db, parent.id).await?.len() as i32 + 1;
    group::ActiveModel {
        project_id: Set(p.id),
        parent_id: Set(Some(parent.id)),
        key: Set(key.to_string()),
        name: Set(name.to_string()),
        description: Set(String::new()),
        sort_order: Set(next), // 新建追加到同层分组末尾
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
    // 外键 CASCADE：子分组（自引用）与其下接口一并删除，单条语句保证原子
    group::Entity::delete_by_id(g.id)
        .exec(db)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// 移动分组到目标分组下（目标路径缺失时自动创建），可指定插到某个同层分组**之前**
/// （`before_key` 为 None 则追加到同层分组末尾）。同层调用即为「拖拽排序」。
/// 落库时把源/目标层内的分组顺序重排为 1..n。
pub async fn move_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    target_group_path: &[String],
    before_key: Option<&str>,
) -> Result<(), String> {
    if group_path.is_empty() {
        return Err("不能移动根级".into());
    }
    if group_path == target_group_path {
        return Err("目标即自身".into());
    }
    // 目标路径若以自身路径为前缀，说明目标在本分组（或其后代）之内
    if group_path.len() <= target_group_path.len()
        && group_path[..] == target_group_path[..group_path.len()]
    {
        return Err("不能移动到自己的子分组下".into());
    }
    let (_, p) = find_project(db, team_key, project_key).await?;
    let g = resolve_group(db, p.id, group_path).await?;
    let key = group_path.last().cloned().unwrap_or_default();
    let src_id = g.id;
    let old_parent = g.parent_id.ok_or("不能移动项目根")?;

    let txn = db.begin().await.map_err(db_err)?;
    let target_parent = ensure_group_path(&txn, p.id, target_group_path).await?;
    // 同父移动时目标键就是自己本身，不算冲突
    if target_parent.id != old_parent && sibling_key_taken(&txn, target_parent.id, &key).await? {
        return Err(format!("目标分组下已存在 {key}"));
    }

    let mut ordered: Vec<i32> = Vec::new();
    let mut placed = false;
    for s in group_siblings(&txn, target_parent.id)
        .await?
        .iter()
        .filter(|s| s.id != src_id)
    {
        if !placed && before_key.is_some_and(|bk| bk == s.key) {
            ordered.push(src_id);
            placed = true;
        }
        ordered.push(s.id);
    }
    if !placed {
        ordered.push(src_id); // 未指定插入点（或目标已不在）→ 同层末尾
    }

    let mut am: group::ActiveModel = g.into();
    am.parent_id = Set(Some(target_parent.id));
    am.update(&txn).await.map_err(db_err)?;
    renumber_groups(&txn, &ordered).await?;
    if target_parent.id != old_parent {
        // 源层可能留下空位：一并重排，保持顺序仍为连续 1..n
        let src: Vec<i32> = group_siblings(&txn, old_parent)
            .await?
            .iter()
            .map(|s| s.id)
            .collect();
        renumber_groups(&txn, &src).await?;
    }
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
        sort_order: Set(next_iface_order(db, parent.id).await?), // 新建追加到分组末尾
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

/// 目标分组内的下一个手动顺序值（新建接口/导入时追加到末尾）
async fn next_iface_order<C: ConnectionTrait>(db: &C, group_id: i32) -> Result<i32, String> {
    let max = iface::Entity::find()
        .filter(iface::Column::GroupId.eq(group_id))
        .order_by_desc(iface::Column::SortOrder)
        .one(db)
        .await
        .map_err(db_err)?
        .map(|m| m.sort_order)
        .unwrap_or(0);
    Ok(max + 1)
}

/// 把一组接口按给定顺序重排为 1..n（拖拽/移动后写这一列）
async fn renumber_ifaces(txn: &DatabaseTransaction, ordered_ids: &[i32]) -> Result<(), String> {
    for (idx, id) in ordered_ids.iter().enumerate() {
        iface::Entity::update_many()
            .col_expr(iface::Column::SortOrder, Expr::value(idx as i32 + 1))
            .filter(iface::Column::Id.eq(*id))
            .exec(txn)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

/// 移动接口到目标分组（目标分组缺失时自动创建），可指定插入到某个同组接口**之前**
/// （`before_key` 为 None 则追加到末尾）。同分组调用即为「拖拽排序」。
/// 落库时把目标分组内的接口顺序重排为 1..n。返回接口键。
pub async fn move_interface(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
    target_group_path: &[String],
    before_key: Option<&str>,
) -> Result<String, String> {
    let (_, _, row) = find_iface_row(db, team_key, project_key, group_path, iface_key).await?;
    let src_id = row.id;
    let txn = db.begin().await.map_err(db_err)?;
    let target = ensure_group_path(&txn, row.project_id, target_group_path).await?;
    let cross_group = target.id != row.group_id;
    if cross_group {
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
    }

    // 目标分组内其余接口（自带顺序：sort_order 升序，同值按 key）
    let mut siblings = iface::Entity::find()
        .filter(iface::Column::GroupId.eq(target.id))
        .filter(iface::Column::Id.ne(src_id))
        .all(&txn)
        .await
        .map_err(db_err)?;
    siblings.sort_by(|a, b| (a.sort_order, &a.key).cmp(&(b.sort_order, &b.key)));

    let mut ordered: Vec<i32> = Vec::with_capacity(siblings.len() + 1);
    let mut placed = false;
    for s in &siblings {
        if !placed && before_key.is_some_and(|bk| bk == s.key) {
            ordered.push(src_id);
            placed = true;
        }
        ordered.push(s.id);
    }
    if !placed {
        ordered.push(src_id); // 未指定插入点（或目标已不在）→ 末尾
    }

    let mut am: iface::ActiveModel = row.into();
    am.group_id = Set(target.id);
    am.update(&txn).await.map_err(db_err)?;
    renumber_ifaces(&txn, &ordered).await?;
    txn.commit().await.map_err(db_err)?;
    Ok(iface_key.to_string())
}

// ----- 快捷请求 -----

async fn find_quick_request_row(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
) -> Result<quick_request::Model, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    quick_request::Entity::find_by_id(id)
        .filter(quick_request::Column::ProjectId.eq(p.id))
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| format!("快捷请求 {id} 不存在"))
}

fn quick_request_summary(m: &quick_request::Model) -> QuickRequestSummary {
    QuickRequestSummary {
        id: m.id,
        name: m.name.clone(),
        method: m.method.clone(),
        url: m.url.clone(),
        group_id: m.group_id,
    }
}

fn quick_group_summary(m: &quick_group::Model) -> QuickGroupSummary {
    QuickGroupSummary {
        id: m.id,
        name: m.name.clone(),
        parent_id: m.parent_id,
    }
}

/// 项目下的快捷请求整棵树（分组按创建顺序、请求按最新在前）。
/// 项目不存在时返回空树（与接口树一致）。
pub async fn list_quick_tree(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> QuickTree {
    let Ok((_, p)) = find_project(db, team_key, project_key).await else {
        return QuickTree::default();
    };
    // 分组按手动顺序（拖拽排序写 sort_order），同值回退到创建顺序
    let Ok(groups) = quick_group::Entity::find()
        .filter(quick_group::Column::ProjectId.eq(p.id))
        .order_by_asc(quick_group::Column::SortOrder)
        .order_by_asc(quick_group::Column::CreatedAtMs)
        .order_by_asc(quick_group::Column::Id)
        .all(db)
        .await
    else {
        return QuickTree::default();
    };
    // 请求按手动顺序（拖拽排序写 sort_order），同值再按 id
    let Ok(requests) = quick_request::Entity::find()
        .filter(quick_request::Column::ProjectId.eq(p.id))
        .order_by_asc(quick_request::Column::SortOrder)
        .order_by_asc(quick_request::Column::Id)
        .all(db)
        .await
    else {
        return QuickTree::default();
    };
    QuickTree {
        groups: groups.iter().map(quick_group_summary).collect(),
        requests: requests.iter().map(quick_request_summary).collect(),
    }
}

/// 校验分组归属（可空 = 顶层）
async fn check_quick_group(
    db: &DatabaseConnection,
    project_id: i32,
    group_id: Option<i64>,
) -> Result<(), String> {
    if let Some(gid) = group_id {
        let exists = quick_group::Entity::find_by_id(gid)
            .filter(quick_group::Column::ProjectId.eq(project_id))
            .one(db)
            .await
            .map_err(db_err)?;
        if exists.is_none() {
            return Err(format!("快捷请求分组 {gid} 不存在"));
        }
    }
    Ok(())
}

/// 目标分组内的下一个手动顺序值（新建/复制时追加到末尾）
async fn next_quick_order(
    db: &impl ConnectionTrait,
    project_id: i32,
    group_id: Option<i64>,
) -> Result<i32, String> {
    let mut q = quick_request::Entity::find()
        .filter(quick_request::Column::ProjectId.eq(project_id));
    q = match group_id {
        Some(g) => q.filter(quick_request::Column::GroupId.eq(g)),
        None => q.filter(quick_request::Column::GroupId.is_null()),
    };
    let max = q
        .order_by_desc(quick_request::Column::SortOrder)
        .one(db)
        .await
        .map_err(db_err)?
        .map(|m| m.sort_order)
        .unwrap_or(0);
    Ok(max + 1)
}

/// 目标分组内的其余快捷请求（按手动顺序，同值按 id）
async fn quick_siblings(
    txn: &DatabaseTransaction,
    project_id: i32,
    group_id: Option<i64>,
    exclude_id: i64,
) -> Result<Vec<quick_request::Model>, String> {
    let mut q = quick_request::Entity::find()
        .filter(quick_request::Column::ProjectId.eq(project_id))
        .filter(quick_request::Column::Id.ne(exclude_id));
    q = match group_id {
        Some(g) => q.filter(quick_request::Column::GroupId.eq(g)),
        None => q.filter(quick_request::Column::GroupId.is_null()),
    };
    let mut rows = q.all(txn).await.map_err(db_err)?;
    rows.sort_by(|a, b| (a.sort_order, a.id).cmp(&(b.sort_order, b.id)));
    Ok(rows)
}

async fn find_quick_group_row(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
) -> Result<quick_group::Model, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    quick_group::Entity::find_by_id(id)
        .filter(quick_group::Column::ProjectId.eq(p.id))
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| format!("快捷请求分组 {id} 不存在"))
}

/// 同层快捷请求分组的显示顺序（手动顺序优先，未排序时回退到创建顺序）
async fn quick_group_siblings<C: ConnectionTrait>(
    db: &C,
    project_id: i32,
    parent_id: Option<i64>,
) -> Result<Vec<quick_group::Model>, String> {
    let mut q = quick_group::Entity::find().filter(quick_group::Column::ProjectId.eq(project_id));
    q = match parent_id {
        Some(pid) => q.filter(quick_group::Column::ParentId.eq(pid)),
        None => q.filter(quick_group::Column::ParentId.is_null()),
    };
    let mut rows = q.all(db).await.map_err(db_err)?;
    rows.sort_by(|a, b| {
        (a.sort_order, a.created_at_ms, a.id).cmp(&(b.sort_order, b.created_at_ms, b.id))
    });
    Ok(rows)
}

/// 把同层快捷请求分组按当前显示顺序重排为 1..n
async fn renumber_quick_groups<C: ConnectionTrait>(db: &C, ordered: &[i64]) -> Result<(), String> {
    for (idx, id) in ordered.iter().enumerate() {
        quick_group::Entity::update_many()
            .col_expr(quick_group::Column::SortOrder, Expr::value(idx as i32 + 1))
            .filter(quick_group::Column::Id.eq(*id))
            .exec(db)
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

/// 补齐旧数据的 0 值顺序（新建分组前调用，保证「新建追加到同层末尾」成立）
async fn normalize_quick_group_order<C: ConnectionTrait>(
    db: &C,
    project_id: i32,
    parent_id: Option<i64>,
) -> Result<(), String> {
    let rows = quick_group_siblings(db, project_id, parent_id).await?;
    if rows
        .iter()
        .enumerate()
        .all(|(i, r)| r.sort_order == i as i32 + 1)
    {
        return Ok(());
    }
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    renumber_quick_groups(db, &ids).await
}

/// 新建快捷请求分组（名称为空时自动取名）；**不校验唯一性**（允许重名）
pub async fn create_quick_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    parent_id: Option<i64>,
    name: &str,
) -> Result<QuickGroupSummary, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    check_quick_group(db, p.id, parent_id).await?;
    let name = if name.trim().is_empty() {
        let count = quick_group::Entity::find()
            .filter(quick_group::Column::ProjectId.eq(p.id))
            .count(db)
            .await
            .map_err(db_err)?;
        format!("新建分组 {}", count + 1)
    } else {
        validate_label(name)?
    };
    normalize_quick_group_order(db, p.id, parent_id).await?;
    let next = quick_group_siblings(db, p.id, parent_id).await?.len() as i32 + 1;
    let model = quick_group::ActiveModel {
        project_id: Set(p.id),
        parent_id: Set(parent_id),
        name: Set(name),
        created_at_ms: Set(now_unix_ms()),
        sort_order: Set(next), // 新建追加到同层分组末尾
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)?;
    Ok(quick_group_summary(&model))
}

/// 快捷请求分组及其全部后代 id（用于环检测）
async fn quick_group_subtree<C: ConnectionTrait>(
    db: &C,
    project_id: i32,
    root: i64,
) -> Result<Vec<i64>, String> {
    let all = quick_group::Entity::find()
        .filter(quick_group::Column::ProjectId.eq(project_id))
        .all(db)
        .await
        .map_err(db_err)?;
    let mut ids = vec![root];
    let mut i = 0;
    while i < ids.len() {
        let cur = ids[i];
        for g in all.iter().filter(|g| g.parent_id == Some(cur)) {
            if !ids.contains(&g.id) {
                ids.push(g.id);
            }
        }
        i += 1;
    }
    Ok(ids)
}

/// 移动快捷请求分组：改父（`None` = 顶层）并可插到某个同层分组**之前**
/// （`before_id` 为 None 则追加到同层末尾）；同层调用即拖拽排序。
/// 分组下请求通过 group_id 关联，移动分组不影响其归属。
pub async fn move_quick_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
    parent_id: Option<i64>,
    before_id: Option<i64>,
) -> Result<(), String> {
    let row = find_quick_group_row(db, team_key, project_key, id).await?;
    if parent_id == Some(id) {
        return Err("目标即自身".into());
    }
    let (_, p) = find_project(db, team_key, project_key).await?;
    check_quick_group(db, p.id, parent_id).await?;
    let subtree = quick_group_subtree(db, p.id, id).await?;
    if parent_id.is_some_and(|pid| subtree.contains(&pid)) {
        return Err("不能移动到自己的子分组下".into());
    }

    let old_parent = row.parent_id;
    let txn = db.begin().await.map_err(db_err)?;
    let mut ordered: Vec<i64> = Vec::new();
    let mut placed = false;
    for s in quick_group_siblings(&txn, row.project_id, parent_id)
        .await?
        .iter()
        .filter(|s| s.id != id)
    {
        if !placed && before_id.is_some_and(|bid| bid == s.id) {
            ordered.push(id);
            placed = true;
        }
        ordered.push(s.id);
    }
    if !placed {
        ordered.push(id); // 未指定插入点（或目标已不在）→ 同层末尾
    }

    let mut am: quick_group::ActiveModel = row.into();
    am.parent_id = Set(parent_id);
    am.update(&txn).await.map_err(db_err)?;
    renumber_quick_groups(&txn, &ordered).await?;
    if parent_id != old_parent {
        let src: Vec<i64> = quick_group_siblings(&txn, p.id, old_parent)
            .await?
            .iter()
            .map(|s| s.id)
            .collect();
        renumber_quick_groups(&txn, &src).await?;
    }
    txn.commit().await.map_err(db_err)?;
    Ok(())
}

/// 重命名快捷请求分组（仅改显示名，无唯一键要求），返回新名称
pub async fn rename_quick_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
    new_name: &str,
) -> Result<String, String> {
    let new_name = validate_label(new_name)?;
    let row = find_quick_group_row(db, team_key, project_key, id).await?;
    let mut am: quick_group::ActiveModel = row.into();
    am.name = Set(new_name.clone());
    am.update(db).await.map_err(db_err)?;
    Ok(new_name)
}

/// 删除快捷请求分组：子分组与其下快捷请求由外键级联删除
pub async fn delete_quick_group(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
) -> Result<(), String> {
    let row = find_quick_group_row(db, team_key, project_key, id).await?;
    quick_group::Entity::delete_by_id(row.id)
        .exec(db)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// 新建快捷请求：名称为空时自动取名「快捷请求 N」；**不校验唯一性**（允许重名）。
/// `group_id` 为空即未分组（快捷请求根层）。
pub async fn create_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_id: Option<i64>,
    name: &str,
) -> Result<QuickRequestSummary, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    check_quick_group(db, p.id, group_id).await?;
    let name = if name.trim().is_empty() {
        let count = quick_request::Entity::find()
            .filter(quick_request::Column::ProjectId.eq(p.id))
            .count(db)
            .await
            .map_err(db_err)?;
        format!("快捷请求 {}", count + 1)
    } else {
        validate_label(name)?
    };
    let mut doc = InterfaceFile::new(&name);
    doc.name = name.clone();
    let model = quick_request::ActiveModel {
        project_id: Set(p.id),
        group_id: Set(group_id),
        name: Set(name.clone()),
        method: Set(doc.method.clone()),
        url: Set(doc.url.clone()),
        sort_order: Set(next_quick_order(db, p.id, group_id).await?), // 新建追加到末尾
        doc: Set(to_json(&doc)?),
        created_at_ms: Set(now_unix_ms()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)?;
    Ok(quick_request_summary(&model))
}

/// 取快捷请求的显示名与完整定义（doc.name 与显示名保持同步）
pub async fn get_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
) -> Result<(String, InterfaceFile), String> {
    let row = find_quick_request_row(db, team_key, project_key, id).await?;
    let doc: InterfaceFile = parse_json(&row.doc, InterfaceFile::new(&row.name));
    Ok((row.name, doc))
}

/// 保存快捷请求内容：同步列表冗余列；名称只由 rename 改动，不受文档影响
pub async fn save_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
    iface: &InterfaceFile,
) -> Result<(), String> {
    let row = find_quick_request_row(db, team_key, project_key, id).await?;
    let mut doc = iface.clone();
    doc.name = row.name.clone();
    if let Some(payload) = doc.body.json_example_payload() {
        doc.body.content = payload;
    }
    let mut am: quick_request::ActiveModel = row.into();
    am.method = Set(doc.method.clone());
    am.url = Set(doc.url.clone());
    am.doc = Set(to_json(&doc)?);
    am.update(db).await.map_err(db_err)?;
    Ok(())
}

/// 复制快捷请求：内容同源但拥有独立文档 id，落在同一分组；
/// 名称后接 -copy（无唯一键，重名不会冲突）
pub async fn copy_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
) -> Result<QuickRequestSummary, String> {
    let row = find_quick_request_row(db, team_key, project_key, id).await?;
    let name = format!("{}-copy", row.name);
    let mut doc: InterfaceFile = parse_json(&row.doc, InterfaceFile::new(&name));
    doc.id = uuid::Uuid::new_v4().to_string();
    doc.name = name.clone();
    let model = quick_request::ActiveModel {
        project_id: Set(row.project_id),
        group_id: Set(row.group_id),
        name: Set(name),
        method: Set(doc.method.clone()),
        url: Set(doc.url.clone()),
        sort_order: Set(next_quick_order(db, row.project_id, row.group_id).await?), // 副本追加到末尾
        doc: Set(to_json(&doc)?),
        created_at_ms: Set(now_unix_ms()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)?;
    Ok(quick_request_summary(&model))
}

/// 移动快捷请求到目标分组（`None` = 未分组，回到快捷请求根层），可指定插入到
/// 某个同组快捷请求**之前**（`before_id` 为 None 则追加到末尾）；同分组调用即为拖拽排序。
pub async fn move_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
    group_id: Option<i64>,
    before_id: Option<i64>,
) -> Result<(), String> {
    let row = find_quick_request_row(db, team_key, project_key, id).await?;
    check_quick_group(db, row.project_id, group_id).await?;
    let project_id = row.project_id;
    let txn = db.begin().await.map_err(db_err)?;
    let siblings = quick_siblings(&txn, project_id, group_id, id).await?;

    let mut ordered: Vec<i64> = Vec::with_capacity(siblings.len() + 1);
    let mut placed = false;
    for s in &siblings {
        if !placed && before_id.is_some_and(|bid| bid == s.id) {
            ordered.push(id);
            placed = true;
        }
        ordered.push(s.id);
    }
    if !placed {
        ordered.push(id); // 未指定插入点（或目标已不在）→ 末尾
    }

    let mut am: quick_request::ActiveModel = row.into();
    am.group_id = Set(group_id);
    am.update(&txn).await.map_err(db_err)?;
    for (idx, qid) in ordered.iter().enumerate() {
        quick_request::Entity::update_many()
            .col_expr(quick_request::Column::SortOrder, Expr::value(idx as i32 + 1))
            .filter(quick_request::Column::Id.eq(*qid))
            .exec(&txn)
            .await
            .map_err(db_err)?;
    }
    txn.commit().await.map_err(db_err)?;
    Ok(())
}

/// 重命名快捷请求：只改显示名；无唯一键，允许与其它快捷请求/接口重名
pub async fn rename_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
    new_name: &str,
) -> Result<String, String> {
    let new_name = validate_label(new_name)?;
    let row = find_quick_request_row(db, team_key, project_key, id).await?;
    let mut doc: InterfaceFile = parse_json(&row.doc, InterfaceFile::new(&new_name));
    doc.name = new_name.clone();
    let mut am: quick_request::ActiveModel = row.into();
    am.name = Set(new_name.clone());
    am.doc = Set(to_json(&doc)?);
    am.update(db).await.map_err(db_err)?;
    Ok(new_name)
}

pub async fn delete_quick_request(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    id: i64,
) -> Result<(), String> {
    let row = find_quick_request_row(db, team_key, project_key, id).await?;
    quick_request::Entity::delete_by_id(row.id)
        .exec(db)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// 快捷请求的历史引用主键：只到项目一级（不属于任何分组/接口）
pub async fn quick_request_refs(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
) -> HistoryRefs {
    match find_project(db, team_key, project_key).await {
        Ok((t, p)) => HistoryRefs {
            team_id: Some(t.id),
            project_id: Some(p.id),
            group_id: None,
            iface_id: None,
        },
        Err(_) => HistoryRefs::default(),
    }
}

// ----- 导入 -----

/// 导入条目：分组路径 + 接口键 + 完整定义
pub struct ImportItem {
    pub group_path: Vec<String>,
    pub key: String,
    pub doc: InterfaceFile,
}

/// 事务内批量导入：缺失分组自动创建并复用；单条冲突记 Err 跳过、不影响其余；
/// 仅整体提交失败（连接级错误）时回滚全部，保证不留下半成品。
pub async fn apply_import(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    items: &[ImportItem],
) -> Result<Vec<Result<(), String>>, String> {
    let (_, p) = find_project(db, team_key, project_key).await?;
    let txn = db.begin().await.map_err(db_err)?;
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        let target = match ensure_group_path(&txn, p.id, &item.group_path).await {
            Ok(g) => g,
            Err(e) => {
                results.push(Err(format!("分组创建失败：{e}")));
                continue;
            }
        };
        if sibling_key_taken(&txn, target.id, &item.key).await? {
            results.push(Err(format!("接口键 {} 已被占用", item.key)));
            continue;
        }
        let mut doc = item.doc.clone();
        if let Some(payload) = doc.body.json_example_payload() {
            doc.body.content = payload;
        }
        let sort_order = match next_iface_order(&txn, target.id).await {
            Ok(n) => n,
            Err(e) => {
                results.push(Err(e));
                continue;
            }
        };
        let insert = iface::ActiveModel {
            project_id: Set(p.id),
            group_id: Set(target.id),
            key: Set(item.key.clone()),
            name: Set(doc.name.clone()),
            method: Set(doc.method.clone()),
            sort_order: Set(sort_order),
            doc: Set(to_json(&doc)?),
            ..Default::default()
        }
        .insert(&txn)
        .await;
        results.push(insert.map(|_| ()).map_err(db_err));
    }
    txn.commit().await.map_err(db_err)?;
    Ok(results)
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
        .order_by_asc(environment::Column::EnvId)
        .all(db)
        .await
    else {
        return Vec::new();
    };
    rows.into_iter()
        .map(|e| EnvironmentSummary {
            active: Some(e.env_id.clone()) == active,
            id: e.env_id,
            file: e.file_key,
            name: e.name,
            host: e.host,
            builtin: e.builtin,
        })
        .collect()
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

// ----- 请求历史 -----

/// 新增历史的全部字段（JSON 列由调用方序列化，保持仓储层为纯 SQL 语义）
pub struct HistoryInput {
    pub team_key: String,
    pub project_key: String,
    pub project_name: String,
    pub env_id: String,
    pub env_name: String,
    pub iface_key: String,
    pub iface_name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub ok: bool,
    pub time_ms: u64,
    pub created_at_ms: i64,
    /// 引用主键（可空）
    pub team_id: Option<i32>,
    pub project_id: Option<i32>,
    pub group_id: Option<i32>,
    pub iface_id: Option<i32>,
    pub doc_json: String,
    pub env_json: String,
    pub global_variables_json: String,
    pub global_params_json: String,
    pub response_json: Option<String>,
    pub error_json: Option<String>,
}

/// 按 key 路径定位引用主键（团队 → 项目 → 分组 → 接口）。
/// 从根分组（parent_id 为 NULL、key 为空）沿 group_path 逐级下钻；
/// 任一级找不到即返回已定位部分（尽力而为，不阻断历史记录）。
pub struct HistoryRefs {
    pub team_id: Option<i32>,
    pub project_id: Option<i32>,
    pub group_id: Option<i32>,
    pub iface_id: Option<i32>,
}

impl Default for HistoryRefs {
    fn default() -> Self {
        Self {
            team_id: None,
            project_id: None,
            group_id: None,
            iface_id: None,
        }
    }
}

pub async fn resolve_history_refs(
    db: &DatabaseConnection,
    team_key: &str,
    project_key: &str,
    group_path: &[String],
    iface_key: &str,
) -> HistoryRefs {
    let team = match team::Entity::find()
        .filter(team::Column::Key.eq(team_key))
        .one(db)
        .await
    {
        Ok(Some(t)) => t,
        _ => return HistoryRefs::default(),
    };
    let mut refs = HistoryRefs { team_id: Some(team.id), ..Default::default() };
    let project = match project::Entity::find()
        .filter(project::Column::TeamId.eq(team.id))
        .filter(project::Column::Key.eq(project_key))
        .one(db)
        .await
    {
        Ok(Some(p)) => p,
        _ => return refs,
    };
    refs.project_id = Some(project.id);
    let root = match group::Entity::find()
        .filter(group::Column::ProjectId.eq(project.id))
        .filter(group::Column::ParentId.is_null())
        .one(db)
        .await
    {
        Ok(Some(g)) => g,
        _ => return refs,
    };
    let mut cur = root;
    for seg in group_path {
        if seg.is_empty() {
            continue;
        }
        let Some(next) = group::Entity::find()
            .filter(group::Column::ParentId.eq(cur.id))
            .filter(group::Column::Key.eq(seg))
            .one(db)
            .await
            .ok()
            .flatten()
        else {
            return refs;
        };
        cur = next;
    }
    refs.group_id = Some(cur.id);
    if iface_key.is_empty() {
        return refs;
    }
    let Some(iface_row) = iface::Entity::find()
        .filter(iface::Column::ProjectId.eq(project.id))
        .filter(iface::Column::GroupId.eq(cur.id))
        .filter(iface::Column::Key.eq(iface_key))
        .one(db)
        .await
        .ok()
        .flatten()
    else {
        return refs;
    };
    refs.iface_id = Some(iface_row.id);
    refs
}

pub async fn insert_history(
    db: &DatabaseConnection,
    input: HistoryInput,
) -> Result<request_history::Model, String> {
    request_history::ActiveModel {
        team_key: Set(input.team_key),
        project_key: Set(input.project_key),
        project_name: Set(input.project_name),
        env_id: Set(input.env_id),
        env_name: Set(input.env_name),
        iface_key: Set(input.iface_key),
        iface_name: Set(input.iface_name),
        method: Set(input.method),
        url: Set(input.url),
        status: Set(input.status.map(|s| s as i32)),
        ok: Set(input.ok),
        time_ms: Set(input.time_ms as i64),
        created_at_ms: Set(input.created_at_ms),
        team_id: Set(input.team_id),
        project_id: Set(input.project_id),
        group_id: Set(input.group_id),
        iface_id: Set(input.iface_id),
        doc: Set(input.doc_json),
        env_json: Set(input.env_json),
        global_variables: Set(input.global_variables_json),
        global_params: Set(input.global_params_json),
        response: Set(input.response_json),
        error: Set(input.error_json),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(db_err)
}

/// 列表：按时间倒序（同毫秒再按 id 倒序），仅返回总结
pub async fn list_history(db: &DatabaseConnection) -> Vec<HistorySummary> {
    let Ok(rows) = request_history::Entity::find()
        .order_by_desc(request_history::Column::CreatedAtMs)
        .order_by_desc(request_history::Column::Id)
        .all(db)
        .await
    else {
        return Vec::new();
    };
    rows.iter().map(history_summary).collect()
}

fn history_summary(m: &request_history::Model) -> HistorySummary {
    HistorySummary {
        id: m.id,
        team_key: m.team_key.clone(),
        project_key: m.project_key.clone(),
        project_name: m.project_name.clone(),
        env_id: m.env_id.clone(),
        env_name: m.env_name.clone(),
        iface_key: m.iface_key.clone(),
        iface_name: m.iface_name.clone(),
        team_id: m.team_id,
        project_id: m.project_id,
        group_id: m.group_id,
        iface_id: m.iface_id,
        method: m.method.clone(),
        url: m.url.clone(),
        status: m.status.map(|s| s as u16),
        ok: m.ok,
        time_ms: m.time_ms.max(0) as u64,
        created_at_ms: m.created_at_ms,
    }
}

pub async fn get_history(
    db: &DatabaseConnection,
    id: i64,
) -> Result<HistoryRecord, String> {
    let row = request_history::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| format!("历史记录 {id} 不存在"))?;
    Ok(history_record(&row))
}

/// 模型 → 完整记录（快照列宽容解析：损坏时回落空值，不阻塞查看/重发）
pub fn history_record(m: &request_history::Model) -> HistoryRecord {
    HistoryRecord {
        id: m.id,
        team_key: m.team_key.clone(),
        project_key: m.project_key.clone(),
        project_name: m.project_name.clone(),
        env_id: m.env_id.clone(),
        env_name: m.env_name.clone(),
        iface_key: m.iface_key.clone(),
        iface_name: m.iface_name.clone(),
        team_id: m.team_id,
        project_id: m.project_id,
        group_id: m.group_id,
        iface_id: m.iface_id,
        method: m.method.clone(),
        url: m.url.clone(),
        status: m.status.map(|s| s as u16),
        ok: m.ok,
        time_ms: m.time_ms.max(0) as u64,
        created_at_ms: m.created_at_ms,
        doc: parse_json(&m.doc, InterfaceFile::new("")),
        env: parse_json(&m.env_json, EnvironmentFile {
            version: SCHEMA_VERSION,
            id: String::new(),
            file: String::new(),
            name: String::new(),
            host: String::new(),
            builtin: false,
            variables: Vec::new(),
        }),
        global_variables: parse_json(&m.global_variables, Vec::new()),
        global_params: parse_json(&m.global_params, GlobalParams::default()),
        response: m
            .response
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok()),
        error: m
            .error
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok()),
    }
}

pub async fn delete_history(db: &DatabaseConnection, id: i64) -> Result<(), String> {
    request_history::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(db_err)?;
    Ok(())
}

pub async fn clear_history(db: &DatabaseConnection) -> Result<(), String> {
    request_history::Entity::delete_many()
        .exec(db)
        .await
        .map_err(db_err)?;
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
        let new_key = move_interface(&db, "运维研发", "用户中心", &[], "a", &["g1".to_string()], None).await.unwrap();
        assert_eq!(new_key, "a");
        assert!(get_interface(&db, "运维研发", "用户中心", &["g1".to_string()], "a").await.is_ok());
        assert!(get_interface(&db, "运维研发", "用户中心", &[], "a").await.is_err());

        // 移动分组 g1 到 ext 下
        create_group(&db, "运维研发", "用户中心", &[], "ext", "外部").await.unwrap();
        move_group(&db, "运维研发", "用户中心", &["g1".to_string()], &["ext".to_string()], None).await.unwrap();
        // 子树随行移动
        assert!(get_interface(&db, "运维研发", "用户中心", &["ext".to_string(), "g1".to_string()], "in-g1").await.is_ok());
        assert!(get_interface(&db, "运维研发", "用户中心", &["g1".to_string()], "in-g1").await.is_err());

        // 不能移动到自身子孙
        assert!(move_group(&db, "运维研发", "用户中心", &["ext".to_string()], &["ext".to_string(), "g1".to_string()], None).await.is_err());
        // 同层移动（before_key 指向自己）不应被当成键冲突
        move_group(&db, "运维研发", "用户中心", &["ext".to_string()], &[], Some("ext")).await.unwrap();
        move_group(&db, "运维研发", "用户中心", &["ext".to_string()], &[], None).await.unwrap();

        // 分组重命名（改键）
        rename_group(&db, "运维研发", "用户中心", &["ext".to_string()], "外部服务").await.unwrap();
        let tree = list_interface_tree(&db, "运维研发", "用户中心").await;
        assert!(matches!(&tree.iter().find(|n| matches!(n, TreeNode::Group { key, .. } if key == "外部服务")), Some(_)));

        // 删除项目级联清空
        delete_project(&db, "运维研发", "用户中心").await.unwrap();
        assert!(list_projects(&db, "运维研发").await.is_empty());
    }

    #[tokio::test]
    async fn quick_request_crud_allows_duplicate_names() {
        let db = setup("quick").await;
        assert!(list_quick_tree(&db, "ops", "user-api").await.requests.is_empty());

        // 名称为空 → 自动取名；不要求唯一
        let a = create_quick_request(&db, "ops", "user-api", None, "").await.unwrap();
        assert_eq!(a.name, "快捷请求 1");
        assert!(a.group_id.is_none());
        let b = create_quick_request(&db, "ops", "user-api", None, "").await.unwrap();
        assert_eq!(b.name, "快捷请求 2");
        // 显式重名：与接口/分组不同，允许
        let c1 = create_quick_request(&db, "ops", "user-api", None, "登录调试").await.unwrap();
        let c2 = create_quick_request(&db, "ops", "user-api", None, "登录调试").await.unwrap();
        assert_eq!(c1.name, c2.name);
        assert_eq!(list_quick_tree(&db, "ops", "user-api").await.requests.len(), 4);

        // 保存内容：method/url 同步到列表列，名称不受文档影响
        let (_, mut doc) = get_quick_request(&db, "ops", "user-api", c1.id).await.unwrap();
        doc.method = "POST".into();
        doc.url = "http://127.0.0.1:8080/api/login".into();
        doc.name = "不该生效的名字".into();
        save_quick_request(&db, "ops", "user-api", c1.id, &doc).await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        let row = tree.requests.iter().find(|q| q.id == c1.id).unwrap();
        assert_eq!(row.method, "POST");
        assert_eq!(row.url, "http://127.0.0.1:8080/api/login");
        assert_eq!(row.name, "登录调试");
        let (name, doc2) = get_quick_request(&db, "ops", "user-api", c1.id).await.unwrap();
        assert_eq!(name, "登录调试");
        assert_eq!(doc2.name, "登录调试");

        // 重命名（无唯一键要求，可与其它快捷请求重名）
        let new_name = rename_quick_request(&db, "ops", "user-api", c2.id, "登录调试").await.unwrap();
        assert_eq!(new_name, "登录调试");
        assert!(rename_quick_request(&db, "ops", "user-api", c2.id, "  ").await.is_err());

        // 删除 + 项目级联
        delete_quick_request(&db, "ops", "user-api", b.id).await.unwrap();
        assert_eq!(list_quick_tree(&db, "ops", "user-api").await.requests.len(), 3);
        assert!(get_quick_request(&db, "ops", "user-api", b.id).await.is_err());
        delete_project(&db, "ops", "user-api").await.unwrap();
        assert!(list_quick_tree(&db, "ops", "user-api").await.requests.is_empty());
    }

    #[tokio::test]
    async fn quick_groups_nest_allow_duplicates_and_cascade() {
        let db = setup("quickgrp").await;
        let g1 = create_quick_group(&db, "ops", "user-api", None, "登录").await.unwrap();
        // 自动取名按当前分组数递增
        let g3 = create_quick_group(&db, "ops", "user-api", None, "").await.unwrap();
        assert_eq!(g3.name, "新建分组 2");
        // 嵌套 + 重名（无唯一键）
        let g2 = create_quick_group(&db, "ops", "user-api", Some(g1.id), "子分组").await.unwrap();
        assert_eq!(g2.parent_id, Some(g1.id));
        let g4 = create_quick_group(&db, "ops", "user-api", None, "登录").await.unwrap();
        assert_eq!(g4.name, "登录");

        // 分组内新建快捷请求
        let r = create_quick_request(&db, "ops", "user-api", Some(g2.id), "").await.unwrap();
        assert_eq!(r.group_id, Some(g2.id));
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(tree.groups.len(), 4);
        assert_eq!(tree.requests.len(), 1);
        assert_eq!(tree.requests[0].group_id, Some(g2.id));

        // 分组必须属于本项目
        assert!(create_quick_request(&db, "ops", "user-api", Some(9999), "").await.is_err());
        assert!(create_quick_group(&db, "ops", "user-api", Some(9999), "x").await.is_err());

        // 重命名分组（可重名、不可空）
        assert_eq!(
            rename_quick_group(&db, "ops", "user-api", g1.id, "登录相关").await.unwrap(),
            "登录相关"
        );
        assert!(rename_quick_group(&db, "ops", "user-api", g1.id, "   ").await.is_err());

        // 删除父分组：子分组与其下快捷请求级联删除，其它分组不受影响
        delete_quick_group(&db, "ops", "user-api", g1.id).await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(tree.groups.len(), 2);
        assert!(tree.groups.iter().all(|g| g.id != g2.id));
        assert!(tree.requests.is_empty());
        assert!(delete_quick_group(&db, "ops", "user-api", g1.id).await.is_err());

        // 项目级联删除分组
        delete_project(&db, "ops", "user-api").await.unwrap();
        assert!(list_quick_tree(&db, "ops", "user-api").await.groups.is_empty());
    }

    #[tokio::test]
    async fn quick_request_copy_has_own_doc_and_move_changes_group() {
        let db = setup("quickcopy").await;
        let g = create_quick_group(&db, "ops", "user-api", None, "登录").await.unwrap();
        let r = create_quick_request(&db, "ops", "user-api", Some(g.id), "请求A").await.unwrap();

        // 复制：同分组、名称后接 -copy、文档 id 独立
        let copy = copy_quick_request(&db, "ops", "user-api", r.id).await.unwrap();
        assert_eq!(copy.name, "请求A-copy");
        assert_eq!(copy.group_id, Some(g.id));
        let (_, src) = get_quick_request(&db, "ops", "user-api", r.id).await.unwrap();
        let (_, dup) = get_quick_request(&db, "ops", "user-api", copy.id).await.unwrap();
        assert_ne!(src.id, dup.id);
        assert_eq!(dup.name, "请求A-copy");

        // 移动：到根层再回到分组；目标分组必须属于本项目
        move_quick_request(&db, "ops", "user-api", copy.id, None, None).await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(tree.requests.iter().find(|q| q.id == copy.id).unwrap().group_id, None);
        move_quick_request(&db, "ops", "user-api", copy.id, Some(g.id), None).await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(tree.requests.iter().find(|q| q.id == copy.id).unwrap().group_id, Some(g.id));
        assert!(move_quick_request(&db, "ops", "user-api", copy.id, Some(9999), None).await.is_err());
        assert!(move_quick_request(&db, "ops", "user-api", 9999, None, None).await.is_err());
    }

    /// 树内接口顺序：新建追加到末尾，拖拽（同分组 move + before_key）可重排
    #[tokio::test]
    async fn interface_tree_order_follows_sort_order() {
        let db = setup("order").await;
        for k in ["a", "b", "c"] {
            create_interface(&db, "ops", "user-api", &[], k, k).await.unwrap();
        }
        let keys = |tree: Vec<TreeNode>| -> Vec<String> {
            tree.into_iter()
                .filter_map(|n| match n {
                    TreeNode::Interface { key, .. } => Some(key),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(keys(list_interface_tree(&db, "ops", "user-api").await), vec!["a", "b", "c"]);

        // c 拖到 a 之前（同分组 → 等价于拖拽排序）
        move_interface(&db, "ops", "user-api", &[], "c", &[], Some("a")).await.unwrap();
        assert_eq!(keys(list_interface_tree(&db, "ops", "user-api").await), vec!["c", "a", "b"]);

        // b 拖到末尾
        move_interface(&db, "ops", "user-api", &[], "b", &[], None).await.unwrap();
        assert_eq!(keys(list_interface_tree(&db, "ops", "user-api").await), vec!["c", "a", "b"]);

        // 分组恒在同层接口之前
        create_group(&db, "ops", "user-api", &[], "z-group", "末尾分组").await.unwrap();
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert!(matches!(&tree[0], TreeNode::Group { key, .. } if key == "z-group"));
        assert_eq!(keys(tree).len(), 3);

        // 拖进分组：目标分组内顺序由 before_key 决定
        create_interface(&db, "ops", "user-api", &["z-group".to_string()], "inner", "内部").await.unwrap();
        move_interface(
            &db,
            "ops",
            "user-api",
            &[],
            "c",
            &["z-group".to_string()],
            Some("inner"),
        )
        .await
        .unwrap();
        let inner = get_interface(&db, "ops", "user-api", &["z-group".to_string()], "c").await;
        assert!(inner.is_err() == false);
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        let group = tree
            .iter()
            .find_map(|n| match n {
                TreeNode::Group { children, .. } => Some(children.clone()),
                _ => None,
            })
            .unwrap();
        let inner_keys: Vec<String> = group
            .into_iter()
            .filter_map(|n| match n {
                TreeNode::Interface { key, .. } => Some(key),
                _ => None,
            })
            .collect();
        assert_eq!(inner_keys, vec!["c", "inner"]);
    }

    /// 快捷请求拖拽排序：同分组 move + before_id
    #[tokio::test]
    async fn quick_request_order_follows_sort_order() {
        let db = setup("quickorder").await;
        let a = create_quick_request(&db, "ops", "user-api", None, "a").await.unwrap();
        let b = create_quick_request(&db, "ops", "user-api", None, "b").await.unwrap();
        let c = create_quick_request(&db, "ops", "user-api", None, "c").await.unwrap();
        let names = |tree: QuickTree| -> Vec<String> {
            tree.requests.into_iter().map(|r| r.name).collect()
        };
        assert_eq!(names(list_quick_tree(&db, "ops", "user-api").await), vec!["a", "b", "c"]);

        // c 拖到 a 之前
        move_quick_request(&db, "ops", "user-api", c.id, None, Some(a.id)).await.unwrap();
        assert_eq!(names(list_quick_tree(&db, "ops", "user-api").await), vec!["c", "a", "b"]);

        // b 拖到末尾
        move_quick_request(&db, "ops", "user-api", b.id, None, None).await.unwrap();
        assert_eq!(names(list_quick_tree(&db, "ops", "user-api").await), vec!["c", "a", "b"]);

        // 拖进分组：各组内各自有序（前端按 groupId 归位，平铺顺序不参与展示）
        let g = create_quick_group(&db, "ops", "user-api", None, "分组").await.unwrap();
        move_quick_request(&db, "ops", "user-api", a.id, Some(g.id), None).await.unwrap();
        let in_group = |tree: &QuickTree, gid: Option<i64>| -> Vec<String> {
            tree.requests
                .iter()
                .filter(|r| r.group_id == gid)
                .map(|r| r.name.clone())
                .collect()
        };
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(in_group(&tree, None), vec!["c", "b"]);
        assert_eq!(in_group(&tree, Some(g.id)), vec!["a"]);

        // 组内拖拽排序：d 新建追加在 a 之后，再拖到 a 之前
        let d = create_quick_request(&db, "ops", "user-api", Some(g.id), "d").await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(in_group(&tree, Some(g.id)), vec!["a", "d"]);
        move_quick_request(&db, "ops", "user-api", d.id, Some(g.id), Some(a.id)).await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(in_group(&tree, Some(g.id)), vec!["d", "a"]);
        // 根层顺序不受组内拖拽影响
        assert_eq!(in_group(&tree, None), vec!["c", "b"]);
    }

    /// 根层分组键顺序（用于校验分组拖拽排序）
    fn root_group_keys(tree: &[TreeNode]) -> Vec<String> {
        tree.iter()
            .filter_map(|n| match n {
                TreeNode::Group { key, .. } => Some(key.clone()),
                _ => None,
            })
            .collect()
    }

    /// 接口分组拖拽排序：同层 move + before_key，且分组始终排在接口之前
    #[tokio::test]
    async fn group_order_follows_sort_order() {
        let db = setup("grouporder").await;
        create_group(&db, "ops", "user-api", &[], "a", "a").await.unwrap();
        create_group(&db, "ops", "user-api", &[], "b", "b").await.unwrap();
        create_group(&db, "ops", "user-api", &[], "c", "c").await.unwrap();
        create_interface(&db, "ops", "user-api", &[], "i1", "i1").await.unwrap();

        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert_eq!(root_group_keys(&tree), vec!["a", "b", "c"]);
        // 分组恒定排在接口之前（接口层不影响分组顺序）
        assert_eq!(tree.len(), 4);

        // c 拖到 a 之前（同层排序）
        move_group(&db, "ops", "user-api", &["c".to_string()], &[], Some("a")).await.unwrap();
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert_eq!(root_group_keys(&tree), vec!["c", "a", "b"]);

        // b 拖到末尾
        move_group(&db, "ops", "user-api", &["b".to_string()], &[], None).await.unwrap();
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert_eq!(root_group_keys(&tree), vec!["c", "a", "b"]);

        // 嵌套：a 拖进 b，子树与接口路径随之变化
        create_interface(&db, "ops", "user-api", &["a".to_string()], "in-a", "in-a").await.unwrap();
        move_group(&db, "ops", "user-api", &["a".to_string()], &["b".to_string()], None).await.unwrap();
        let tree = list_interface_tree(&db, "ops", "user-api").await;
        assert_eq!(root_group_keys(&tree), vec!["c", "b"]);
        assert!(get_interface(&db, "ops", "user-api", &["b".to_string(), "a".to_string()], "in-a").await.is_ok());
        assert!(move_group(&db, "ops", "user-api", &["b".to_string()], &["b".to_string(), "a".to_string()], None).await.is_err());
    }

    /// 快捷请求分组拖拽：同层排序、改父、环检测与级联
    #[tokio::test]
    async fn quick_group_order_and_move() {
        let db = setup("quickgrouporder").await;
        let a = create_quick_group(&db, "ops", "user-api", None, "a").await.unwrap();
        let b = create_quick_group(&db, "ops", "user-api", None, "b").await.unwrap();
        let c = create_quick_group(&db, "ops", "user-api", None, "c").await.unwrap();
        let names = |tree: QuickTree| tree.groups.into_iter().map(|g| g.name).collect::<Vec<_>>();
        assert_eq!(names(list_quick_tree(&db, "ops", "user-api").await), vec!["a", "b", "c"]);

        // c 拖到 a 之前
        move_quick_group(&db, "ops", "user-api", c.id, None, Some(a.id)).await.unwrap();
        assert_eq!(names(list_quick_tree(&db, "ops", "user-api").await), vec!["c", "a", "b"]);

        // b 拖进 c（改父），顶层只剩 c、a；c 下新增 b
        move_quick_group(&db, "ops", "user-api", b.id, Some(c.id), None).await.unwrap();
        let tree = list_quick_tree(&db, "ops", "user-api").await;
        assert_eq!(tree.groups.iter().filter(|g| g.parent_id.is_none()).count(), 2);
        assert_eq!(tree.groups.iter().find(|g| g.id == b.id).unwrap().parent_id, Some(c.id));

        // 不能把 c 移到自己的后代 b 下
        assert!(move_quick_group(&db, "ops", "user-api", c.id, Some(b.id), None).await.is_err());
        // 不能把分组移到自己下
        assert!(move_quick_group(&db, "ops", "user-api", c.id, Some(c.id), None).await.is_err());
        // 目标分组必须属于本项目
        assert!(move_quick_group(&db, "ops", "user-api", c.id, Some(9999), None).await.is_err());

        // 重名分组可并存（无唯一键）
        let dup = create_quick_group(&db, "ops", "user-api", None, "a").await.unwrap();
        assert_eq!(dup.name, "a");
    }

    #[tokio::test]
    async fn quick_request_refs_point_at_project_only() {
        let db = setup("quickrefs").await;
        let q = create_quick_request(&db, "ops", "user-api", None, "").await.unwrap();
        let refs = quick_request_refs(&db, "ops", "user-api").await;
        assert!(refs.team_id.is_some());
        assert!(refs.project_id.is_some());
        // 不属于任何分组/接口
        assert!(refs.group_id.is_none());
        assert!(refs.iface_id.is_none());
        assert!(quick_request_refs(&db, "missing", "user-api").await.team_id.is_none());
        let _ = q;
    }
}
