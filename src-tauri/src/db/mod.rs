//! SQLite 持久化层：连接管理 + sea-orm 实体 + 迁移 + 仓储。

pub mod entity;
pub mod migration;
pub mod repo;

use sea_orm::sqlx::sqlite::{SqliteJournalMode, SqliteSynchronous};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 数据根目录内的数据库文件名
pub const DB_FILE: &str = "apidock.db";

pub fn db_path(root: &Path) -> PathBuf {
    root.join(DB_FILE)
}

/// 打开（或创建）数据根目录内的数据库：建库、设连接选项、跑 schema 迁移。
pub async fn open(root: &Path) -> Result<DatabaseConnection, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("创建数据根目录失败：{e}"))?;
    let path = db_path(root);

    // 占位 URL 仅供 sea_orm/sqlx 识别 SQLite 驱动（url 与 sqlx 都需能解析）；
    // 真实文件名与连接选项经回调直传，规避 URL 拼接对空格/中文/特殊字符的编码问题
    let mut opt = ConnectOptions::new("sqlite://apidock");
    opt.map_sqlx_sqlite_opts(move |opts| {
        opts.filename(&path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_millis(5000))
    });
    // 桌面单用户场景：单连接即可，上述选项均为连接级参数、无需重复 PRAGMA
    opt.max_connections(1)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(10));
    let db = Database::connect(opt)
        .await
        .map_err(|e| format!("打开数据库失败：{e}"))?;

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
