//! SQLite 持久化层：连接管理 + sea-orm 实体 + 迁移 + 仓储。

pub mod entity;
pub mod migration;
pub mod repo;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 数据根目录内的数据库文件名
pub const DB_FILE: &str = "apidock.db";

pub fn db_path(root: &Path) -> PathBuf {
    root.join(DB_FILE)
}

/// 打开（或创建）数据根目录内的数据库：建库、设 PRAGMA、跑 schema 迁移。
pub async fn open(root: &Path) -> Result<DatabaseConnection, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("创建数据根目录失败：{e}"))?;
    let path = db_path(root);

    let url = format!(
        "sqlite://{}?mode=rwc",
        path.display().to_string().replace('\\', "/")
    );
    let mut opt = ConnectOptions::new(url);
    // 桌面单用户场景：单连接即可，PRAGMA 只需设置一次且始终生效
    opt.max_connections(1)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(10));
    let db = Database::connect(opt)
        .await
        .map_err(|e| format!("打开数据库失败：{e}"))?;

    db.execute_unprepared("PRAGMA journal_mode = WAL")
        .await
        .map_err(|e| format!("设置 journal_mode 失败：{e}"))?;
    db.execute_unprepared("PRAGMA synchronous = NORMAL")
        .await
        .map_err(|e| format!("设置 synchronous 失败：{e}"))?;
    db.execute_unprepared("PRAGMA foreign_keys = ON")
        .await
        .map_err(|e| format!("设置 foreign_keys 失败：{e}"))?;
    db.execute_unprepared("PRAGMA busy_timeout = 5000")
        .await
        .map_err(|e| format!("设置 busy_timeout 失败：{e}"))?;

    migration::migrate(&db).await?;
    Ok(db)
}

#[cfg(test)]
pub mod tests_support {
    use super::*;

    /// 在临时目录创建一个全新数据库（走完整的 open 流程）
    pub async fn temp_db(tag: &str) -> DatabaseConnection {
        let dir = std::env::temp_dir().join(format!(
            "apidock-dbtest-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        open(&dir).await.expect("打开测试数据库失败")
    }
}
