pub mod models;
pub mod repository;

use anyhow::Result;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub use repository::Repository;

const MIGRATION_SQL: &str = include_str!("../../migrations/001_initial.sql");

pub fn init_db(database_path: &str) -> Result<Arc<Mutex<Connection>>> {
    let conn = Connection::open(database_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(MIGRATION_SQL)?;
    tracing::info!("Database initialized at {}", database_path);
    Ok(Arc::new(Mutex::new(conn)))
}
