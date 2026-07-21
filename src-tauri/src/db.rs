use rusqlite::{params, Connection, Result, Error as SqliteError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::sync::{Mutex, Once};
use std::ops::{Deref, DerefMut};

static DB_INIT: Once = Once::new();
static mut DB_CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);

fn init_db_connection() {
    DB_INIT.call_once(|| {
        let conn = match connect_inner() {
            Ok(c) => c,
            Err(_) => {
                let db_path = get_db_path();
                let _ = fs::remove_file(&db_path);
                Connection::open(&db_path).unwrap_or_else(|_| {
                    panic!("无法初始化数据库连接");
                })
            }
        };
        #[allow(static_mut_refs)]
        unsafe {
            *DB_CONNECTION.lock().unwrap() = Some(conn);
        }
    });
}

struct DbGuard(std::sync::MutexGuard<'static, Option<Connection>>);

impl Deref for DbGuard {
    type Target = Connection;
    
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl DerefMut for DbGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}

fn get_db_guard() -> Result<DbGuard> {
    init_db_connection();
    #[allow(static_mut_refs)]
    let mutex = unsafe { &DB_CONNECTION };
    let guard = mutex.lock().map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(DbGuard(guard))
}

fn connect_inner() -> Result<Connection> {
    let db_path = get_db_path();
    let conn = Connection::open(&db_path)?;
    if conn.execute("SELECT 1", []).is_ok() {
        create_tables(&conn)?;
        Ok(conn)
    } else {
        let _ = fs::remove_file(&db_path);
        let conn = Connection::open(&db_path)?;
        create_tables(&conn)?;
        Ok(conn)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub text: String,
    pub status: bool,
    pub color: String,
    pub bold: bool,
    pub timer_type: String,
    pub timer_value: i64,
    pub timer_remaining: i64,
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
    pub timer_value: i64,
    pub timer_remaining: i64,
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

#[allow(unused)]
fn is_db_corrupted(error: &SqliteError) -> bool {
    let error_str = error.to_string().to_lowercase();
    error_str.contains("cannot open") || 
    error_str.contains("corrupt") || 
    error_str.contains("io error") || 
    error_str.contains("database disk image is malformed") ||
    error_str.contains("file is encrypted or is not a database")
}

fn sanitize_color(color: &str) -> String {
    let color = color.trim();
    if color.len() == 7 && color.starts_with('#') {
        let hex = &color[1..];
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("#{}", hex.to_uppercase());
        }
    }
    "#000000".to_string()
}

fn connect_with_recovery() -> Result<DbGuard> {
    get_db_guard()
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
    timer_value: i64,
    timer_remaining: i64,
) -> Result<Task> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let created_at = chrono::Local::now().to_rfc3339();
    let safe_color = sanitize_color(color);

    let max_order: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(order_index), -1) FROM tasks WHERE status = ?1",
            [status],
            |row| row.get(0),
        )
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO tasks (text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![text, status, safe_color, bold, timer_type, timer_value, timer_remaining, created_at, max_order + 1],
    )?;

    Ok(Task {
        id: conn.last_insert_rowid(),
        text: text.to_string(),
        status,
        color: safe_color,
        bold,
        timer_type: timer_type.to_string(),
        timer_value,
        timer_remaining,
        created_at,
        order_index: max_order + 1,
    })
}

pub fn get_tasks() -> Result<Vec<Task>> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let mut stmt = conn.prepare("SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks ORDER BY status, order_index")?;
    let tasks_iter = stmt.query_map([], |row| {
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

    let mut tasks = Vec::new();
    for task in tasks_iter {
        tasks.push(task?);
    }

    Ok(tasks)
}

pub fn update_task(
    id: i64,
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i64,
    timer_remaining: i64,
) -> Result<Task> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let safe_color = sanitize_color(color);

    conn.execute(
        "UPDATE tasks SET text = ?1, status = ?2, color = ?3, bold = ?4, timer_type = ?5, timer_value = ?6, timer_remaining = ?7 WHERE id = ?8",
        params![text, status, safe_color, bold, timer_type, timer_value, timer_remaining, id],
    )?;

    let task = conn.query_row(
        "SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks WHERE id = ?1",
        [id],
        |row| {
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
        },
    )?;

    Ok(task)
}

pub fn delete_task(id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let task: Task = conn.query_row(
        "SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks WHERE id = ?1",
        [id],
        |row| {
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
        },
    )?;

    let deleted_at = chrono::Local::now().to_rfc3339();

    conn.execute(
        "INSERT INTO deleted_tasks (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![task.id, task.text, task.status, task.color, task.bold, task.timer_type, task.timer_value, task.timer_remaining, task.created_at, task.order_index, deleted_at],
    )?;

    conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;

    Ok(true)
}

pub fn get_deleted_tasks() -> Result<Vec<DeletedTask>> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let mut stmt = conn.prepare("SELECT id, original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at FROM deleted_tasks ORDER BY deleted_at DESC")?;
    let tasks_iter = stmt.query_map([], |row| {
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

    let mut tasks = Vec::new();
    for task in tasks_iter {
        tasks.push(task?);
    }

    Ok(tasks)
}

pub fn restore_deleted_task(original_id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let deleted_task: DeletedTask = conn.query_row(
        "SELECT id, original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at FROM deleted_tasks WHERE original_id = ?1",
        [original_id],
        |row| {
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
        },
    )?;

    conn.execute(
        "INSERT INTO tasks (text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![deleted_task.text, deleted_task.status, deleted_task.color, deleted_task.bold, deleted_task.timer_type, deleted_task.timer_value, deleted_task.timer_remaining, deleted_task.created_at, deleted_task.order_index],
    )?;

    conn.execute("DELETE FROM deleted_tasks WHERE original_id = ?1", [original_id])?;

    Ok(true)
}

pub fn permanently_delete_task(id: i64) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    conn.execute("DELETE FROM deleted_tasks WHERE id = ?1", [id])?;

    Ok(true)
}

#[allow(unused)]
pub fn clear_deleted_tasks() -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    conn.execute("DELETE FROM deleted_tasks", [])?;

    Ok(true)
}

#[allow(unused)]
pub fn update_task_order(task_id: i64, new_order: i32) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    conn.execute("UPDATE tasks SET order_index = ?1 WHERE id = ?2", params![new_order, task_id])?;

    Ok(true)
}

pub fn get_window_config() -> Result<WindowConfig> {
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

pub fn save_window_config(x: f64, y: f64, height: f64, locked: bool) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    conn.execute(
        "UPDATE window_config SET x = ?1, y = ?2, height = ?3, locked = ?4 WHERE id = 1",
        params![x, y, height, locked],
    )?;

    Ok(true)
}

pub fn save_db_window_config(x: f64, y: f64, height: f64, locked: bool) -> Result<WindowConfig> {
    save_window_config(x, y, height, locked)?;
    Ok(WindowConfig { id: 1, x, y, height, locked })
}

pub fn get_db_window_config() -> Result<WindowConfig> {
    get_window_config()
}

pub fn get_all_tasks() -> Result<Vec<Task>> {
    get_tasks()
}

pub fn move_task_to_trash(task_id: i64) -> Result<bool> {
    delete_task(task_id)
}

pub fn restore_task(deleted_id: i64) -> Result<bool> {
    restore_deleted_task(deleted_id)
}

pub fn delete_completed_tasks() -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE status = 1",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    let tasks: Vec<Task> = conn
        .prepare("SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks WHERE status = 1")?
        .query_map([], |row| {
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
        })?
        .filter_map(|r| r.ok())
        .collect();

    let deleted_at = chrono::Local::now().to_rfc3339();
    for task in tasks {
        conn.execute(
            "INSERT INTO deleted_tasks (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![task.id, task.text, task.status, task.color, task.bold, task.timer_type, task.timer_value, task.timer_remaining, task.created_at, task.order_index, deleted_at],
        ).ok();
    }

    conn.execute("DELETE FROM tasks WHERE status = 1", [])?;
    Ok(count)
}

pub fn delete_all_tasks() -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    let tasks: Vec<Task> = conn
        .prepare("SELECT id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index FROM tasks")?
        .query_map([], |row| {
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
        })?
        .filter_map(|r| r.ok())
        .collect();

    let deleted_at = chrono::Local::now().to_rfc3339();
    for task in tasks {
        conn.execute(
            "INSERT INTO deleted_tasks (original_id, text, status, color, bold, timer_type, timer_value, timer_remaining, created_at, order_index, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![task.id, task.text, task.status, task.color, task.bold, task.timer_type, task.timer_value, task.timer_remaining, task.created_at, task.order_index, deleted_at],
        ).ok();
    }

    conn.execute("DELETE FROM tasks", [])?;
    Ok(count)
}

pub fn move_completed_to_trash() -> Result<i64> {
    delete_completed_tasks()
}

pub fn move_all_to_trash() -> Result<i64> {
    delete_all_tasks()
}

pub fn clear_trash_by_period(period_days: i64) -> Result<i64> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM deleted_tasks WHERE deleted_at < datetime('now', '-?1 days')",
        [period_days],
        |row| row.get(0),
    ).unwrap_or(0);

    conn.execute(
        "DELETE FROM deleted_tasks WHERE deleted_at < datetime('now', '-?1 days')",
        [period_days],
    )?;

    Ok(count)
}

pub fn reorder_tasks(task_ids: Vec<i64>, status: bool) -> Result<bool> {
    let conn = connect_with_recovery()?;
    create_tables(&conn)?;

    for (index, task_id) in task_ids.iter().enumerate() {
        conn.execute(
            "UPDATE tasks SET order_index = ?1 WHERE id = ?2 AND status = ?3",
            params![index as i32, task_id, status],
        )?;
    }

    Ok(true)
}

#[tauri::command]
pub fn insert_task_cmd(
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i64,
    timer_remaining: i64,
) -> Result<Task, String> {
    insert_task(text, status, color, bold, timer_type, timer_value, timer_remaining)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_all_tasks_cmd() -> Result<Vec<Task>, String> {
    get_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_task_cmd(
    id: i64,
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i64,
    timer_remaining: i64,
) -> Result<Task, String> {
    update_task(id, text, status, color, bold, timer_type, timer_value, timer_remaining)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task_cmd(id: i64) -> Result<bool, String> {
    delete_task(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_completed_tasks_cmd() -> Result<i64, String> {
    delete_completed_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_all_tasks_cmd() -> Result<i64, String> {
    delete_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_task_to_trash_cmd(task_id: i64) -> Result<bool, String> {
    move_task_to_trash(task_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_deleted_tasks_cmd() -> Result<Vec<DeletedTask>, String> {
    get_deleted_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_task_cmd(deleted_id: i64) -> Result<bool, String> {
    restore_task(deleted_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn permanently_delete_task_cmd(deleted_id: i64) -> Result<bool, String> {
    permanently_delete_task(deleted_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_trash_by_period_cmd(period_days: i64) -> Result<i64, String> {
    clear_trash_by_period(period_days).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_completed_to_trash_cmd() -> Result<i64, String> {
    move_completed_to_trash().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_all_to_trash_cmd() -> Result<i64, String> {
    move_all_to_trash().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reinitialize_db_cmd() -> Result<bool, String> {
    reinitialize_db().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_tasks_cmd(task_ids: Vec<i64>, status: bool) -> Result<bool, String> {
    reorder_tasks(task_ids, status).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_db_window_config_cmd() -> Result<WindowConfig, String> {
    get_db_window_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_db_window_config_cmd(x: f64, y: f64, height: f64, locked: bool) -> Result<WindowConfig, String> {
    save_db_window_config(x, y, height, locked).map_err(|e| e.to_string())
}