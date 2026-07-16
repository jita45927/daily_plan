use rusqlite::{params, Connection, Result, Error as SqliteError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;


#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub text: String,
    pub status: bool,
    pub color: String,
    pub bold: bool,
    pub timer_type: String,
    pub timer_value: i32,
    pub timer_remaining: i32,
    pub created_at: String,
    pub order_index: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedTask {
    pub id: i64,
    pub original_id: i64,
    pub text: String,
    pub status: bool,
    pub color: String,
    pub bold: bool,
    pub timer_type: String,
    pub timer_value: i32,
    pub timer_remaining: i32,
    pub created_at: String,
    pub order_index: i32,
    pub deleted_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindowConfig {
    pub id: i64,
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub locked: bool,
}

fn get_db_path() -> PathBuf {
    // 使用可执行文件所在目录，避免开发模式下数据库文件被 Tauri 文件监视器检测到
    let mut path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    path.push("data");
    std::fs::create_dir_all(&path).unwrap_or_default();
    path.push("daily_plan.db");
    path
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text TEXT NOT NULL,
            status BOOLEAN NOT NULL DEFAULT 0,
            color TEXT NOT NULL DEFAULT '#000000',
            bold BOOLEAN NOT NULL DEFAULT 0,
            timer_type TEXT NOT NULL DEFAULT '',
            timer_value INTEGER NOT NULL DEFAULT 0,
            timer_remaining INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            order_index INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )?;

    let _ = conn.execute(
        "ALTER TABLE tasks ADD COLUMN order_index INTEGER NOT NULL DEFAULT 0",
        [],
    );

    conn.execute(
        "CREATE TABLE IF NOT EXISTS window_config (
            id INTEGER PRIMARY KEY,
            x REAL NOT NULL DEFAULT 0,
            y REAL NOT NULL DEFAULT 0,
            height REAL NOT NULL DEFAULT 600,
            locked BOOLEAN NOT NULL DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS deleted_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            original_id INTEGER NOT NULL,
            text TEXT NOT NULL,
            status BOOLEAN NOT NULL DEFAULT 0,
            color TEXT NOT NULL DEFAULT '#000000',
            bold BOOLEAN NOT NULL DEFAULT 0,
            timer_type TEXT NOT NULL DEFAULT '',
            timer_value INTEGER NOT NULL DEFAULT 0,
            timer_remaining INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            order_index INTEGER NOT NULL DEFAULT 0,
            deleted_at TEXT NOT NULL
        )",
        [],
    )?;

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM window_config", [], |row| row.get(0))?;
    if count == 0 {
        conn.execute(
            "INSERT INTO window_config (id, x, y, height, locked) VALUES (1, 0, 0, 600, 0)",
            [],
        )?;
    }

    Ok(())
}

fn is_db_corrupted(error: &SqliteError) -> bool {
    let error_str = error.to_string().to_lowercase();
    error_str.contains("cannot open") || 
    error_str.contains("corrupt") || 
    error_str.contains("io error") || 
    error_str.contains("database disk image is malformed") ||
    error_str.contains("file is encrypted or is not a database")
}

fn connect_with_recovery() -> Result<Connection> {
    let db_path = get_db_path();
    
    match Connection::open(&db_path) {
        Ok(conn) => {
            if conn.execute("SELECT 1", []).is_ok() {
                Ok(conn)
            } else {
                if fs::remove_file(&db_path).is_err() {}
                let conn = Connection::open(&db_path)?;
                create_tables(&conn)?;
                Ok(conn)
            }
        }
        Err(e) => {
            if is_db_corrupted(&e) {
                let _ = fs::remove_file(&db_path);
                let conn = Connection::open(&db_path)?;
                create_tables(&conn)?;
                Ok(conn)
            } else {
                Err(e)
            }
        }
    }
}

pub fn reinitialize_db() -> Result<bool> {
    let db_path = get_db_path();
    
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }
    
    let conn = Connection::open(&db_path)?;
    create_tables(&conn)?;
    Ok(true)
}

pub fn insert_task(
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i32,
    timer_remaining: i32,
) -> Result<Task> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let created_at = chrono::Local::now().to_rfc3339();

    let max_order: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(order_index), -1) FROM tasks WHERE status = ?1",
            params![status],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    let order_index = max_order + 1;

    conn.execute(
        "INSERT INTO tasks (text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index],
    )?;

    Ok(Task {
        id: conn.last_insert_rowid(),
        text: text.to_string(),
        status,
        color: color.to_string(),
        bold,
        timer_type: timer_type.to_string(),
        timer_value,
        timer_remaining,
        created_at,
        order_index,
    })
}

pub fn get_all_tasks() -> Result<Vec<Task>> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let mut stmt = conn.prepare("SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks ORDER BY status ASC, order_index ASC, created_at DESC")?;
    let tasks = stmt.query_map([], |row| {
        Ok(Task {
            id: row.get(0)?,
            text: row.get(1)?,
            status: row.get(2)?,
            color: row.get(3)?,
            bold: row.get(4)?,
            timer_type: row.get(5)?,
            timer_value: row.get(6)?,
            timer_remaining: row.get(7)?,
            created_at: row.get(8)?,
            order_index: row.get(9)?,
        })
    })?;

    let mut result = Vec::new();
    for task in tasks {
        result.push(task?);
    }

    Ok(result)
}

pub fn update_task(
    id: i64,
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i32,
    timer_remaining: i32,
) -> Result<Task> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    conn.execute(
        "UPDATE tasks SET text = ?1, status = ?2, color = ?3, bold = ?4, timer_type = ?5, timer_value = ?6, timer_remaining = ?7 WHERE id = ?8",
        params![text, status, color, bold, timer_type, timer_value, timer_remaining, id],
    )?;

    let mut stmt = conn.prepare("SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks WHERE id = ?1")?;
    let task = stmt.query_row(params![id], |row| {
        Ok(Task {
            id: row.get(0)?,
            text: row.get(1)?,
            status: row.get(2)?,
            color: row.get(3)?,
            bold: row.get(4)?,
            timer_type: row.get(5)?,
            timer_value: row.get(6)?,
            timer_remaining: row.get(7)?,
            created_at: row.get(8)?,
            order_index: row.get(9)?,
        })
    })?;

    Ok(task)
}

pub fn reorder_tasks(task_ids: Vec<i64>, status: bool) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let tx = conn.unchecked_transaction()?;
    for (index, &task_id) in task_ids.iter().enumerate() {
        tx.execute(
            "UPDATE tasks SET order_index = ?1 WHERE id = ?2 AND status = ?3",
            params![index as i32, task_id, status],
        )?;
    }
    tx.commit()?;

    Ok(true)
}

pub fn delete_task(id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let changes = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    Ok(changes > 0)
}

pub fn delete_completed_tasks() -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let changes = conn.execute("DELETE FROM tasks WHERE status = 1", [])?;
    Ok(changes as i64)
}

pub fn delete_all_tasks() -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let changes = conn.execute("DELETE FROM tasks", [])?;
    Ok(changes as i64)
}

pub fn get_db_window_config() -> Result<WindowConfig> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let config = conn.query_row(
        "SELECT id, x, y, height, locked FROM window_config WHERE id = 1",
        [],
        |row| {
            Ok(WindowConfig {
                id: row.get(0)?,
                x: row.get(1)?,
                y: row.get(2)?,
                height: row.get(3)?,
                locked: row.get(4)?,
            })
        },
    )?;

    Ok(config)
}

pub fn save_db_window_config(x: f64, y: f64, height: f64, locked: bool) -> Result<WindowConfig> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    conn.execute(
        "UPDATE window_config SET x = ?1, y = ?2, height = ?3, locked = ?4 WHERE id = 1",
        params![x, y, height, locked],
    )?;

    Ok(WindowConfig { id: 1, x, y, height, locked })
}

pub fn move_task_to_trash(task_id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let mut stmt = conn.prepare("SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks WHERE id = ?1")?;
    let task = stmt.query_row(params![task_id], |row| {
        Ok(Task {
            id: row.get(0)?,
            text: row.get(1)?,
            status: row.get(2)?,
            color: row.get(3)?,
            bold: row.get(4)?,
            timer_type: row.get(5)?,
            timer_value: row.get(6)?,
            timer_remaining: row.get(7)?,
            created_at: row.get(8)?,
            order_index: row.get(9)?,
        })
    })?;

    let deleted_at = chrono::Local::now().to_rfc3339();

    conn.execute(
        "INSERT INTO deleted_tasks (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![task.id, task.text, task.status, task.color, task.bold, task.timer_type, task.timer_value, task.timer_remaining, task.created_at, task.order_index, deleted_at],
    )?;

    conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])?;

    Ok(true)
}

pub fn get_deleted_tasks() -> Result<Vec<DeletedTask>> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let mut stmt = conn.prepare("SELECT id, original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at FROM deleted_tasks ORDER BY deleted_at DESC")?;
    let tasks = stmt.query_map([], |row| {
        Ok(DeletedTask {
            id: row.get(0)?,
            original_id: row.get(1)?,
            text: row.get(2)?,
            status: row.get(3)?,
            color: row.get(4)?,
            bold: row.get(5)?,
            timer_type: row.get(6)?,
            timer_value: row.get(7)?,
            timer_remaining: row.get(8)?,
            created_at: row.get(9)?,
            order_index: row.get(10)?,
            deleted_at: row.get(11)?,
        })
    })?;

    let mut result = Vec::new();
    for task in tasks {
        result.push(task?);
    }

    Ok(result)
}

pub fn restore_task(deleted_id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let mut stmt = conn.prepare("SELECT original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM deleted_tasks WHERE id = ?1")?;
    let deleted_task = stmt.query_row(params![deleted_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, bool>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i32>(6)?,
            row.get::<_, i32>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, i32>(9)?,
        ))
    })?;

    let (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index) = deleted_task;

    let max_order: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(order_index), -1) FROM tasks WHERE status = ?1",
            params![status],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    let new_order_index = if order_index > max_order { order_index } else { max_order + 1 };

    conn.execute(
        "INSERT INTO tasks (id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, new_order_index],
    )?;

    conn.execute("DELETE FROM deleted_tasks WHERE id = ?1", params![deleted_id])?;

    Ok(true)
}

pub fn permanently_delete_task(deleted_id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let changes = conn.execute("DELETE FROM deleted_tasks WHERE id = ?1", params![deleted_id])?;
    Ok(changes > 0)
}

pub fn clear_trash_by_period(period_days: i64) -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let changes = if period_days <= 0 {
        conn.execute("DELETE FROM deleted_tasks", [])?
    } else {
        let cutoff = chrono::Local::now() - chrono::Duration::days(period_days);
        let cutoff_str = cutoff.to_rfc3339();
        conn.execute("DELETE FROM deleted_tasks WHERE deleted_at < ?1", params![cutoff_str])?
    };

    Ok(changes as i64)
}

pub fn move_completed_to_trash() -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let deleted_at = chrono::Local::now().to_rfc3339();

    conn.execute(
        "INSERT INTO deleted_tasks (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at)
         SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, ?1
         FROM tasks WHERE status = 1",
        params![deleted_at],
    )?;

    let changes = conn.execute("DELETE FROM tasks WHERE status = 1", [])?;
    Ok(changes as i64)
}

pub fn move_all_to_trash() -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let deleted_at = chrono::Local::now().to_rfc3339();

    conn.execute(
        "INSERT INTO deleted_tasks (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at)
         SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, ?1
         FROM tasks",
        params![deleted_at],
    )?;

    let changes = conn.execute("DELETE FROM tasks", [])?;
    Ok(changes as i64)
}