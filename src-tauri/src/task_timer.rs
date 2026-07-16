use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::time::interval;
use tauri::Emitter;

pub struct TimerManager {
    timers: Arc<Mutex<Vec<Timer>>>,
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

struct Timer {
    task_id: i64,
    timer_type: String,
    target_time: u64,
    is_running: bool,
}

impl TimerManager {
    pub fn new() -> Self {
        TimerManager {
            timers: Arc::new(Mutex::new(Vec::new())),
            handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub fn start_countdown(
        &self,
        app_handle: tauri::AppHandle,
        task_id: i64,
        minutes: i32,
    ) -> Result<String, String> {
        if minutes < 1 || minutes > 1440 {
            return Err("分钟数必须在 1-1440 之间".to_string());
        }

        let seconds = (minutes as u64) * 60;
        let started_at = Self::get_current_timestamp();
        let target_time = started_at + seconds;

        self.stop_timer(task_id);

        let timer = Timer {
            task_id,
            timer_type: "countdown".to_string(),
            target_time,
            is_running: true,
        };

        {
            let mut timers = self.timers.lock().unwrap();
            timers.push(timer);
        }

        let app_handle_clone = app_handle.clone();
        let timers_clone = self.timers.clone();
        let task_id_clone = task_id;

        let handle = spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                let current_time = TimerManager::get_current_timestamp();
                let mut timers = timers_clone.lock().unwrap();

                if let Some(timer) = timers.iter_mut().find(|t| t.task_id == task_id_clone) {
                    if !timer.is_running {
                        break;
                    }

                    let remaining = if current_time >= timer.target_time {
                        0
                    } else {
                        timer.target_time - current_time
                    };

                    let hours = remaining / 3600;
                    let minutes = (remaining % 3600) / 60;
                    let seconds = remaining % 60;

                    let _ = app_handle_clone.emit(
                        "timer_update",
                        serde_json::json!({
                            "task_id": task_id_clone,
                            "remaining": remaining,
                            "hours": hours,
                            "minutes": minutes,
                            "seconds": seconds,
                            "formatted": format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                        })
                    );

                    if remaining == 0 {
                        timer.is_running = false;
                        let _ = app_handle_clone.emit(
                            "timer_expired",
                            serde_json::json!({
                                "task_id": task_id_clone,
                                "timerType": "countdown"
                            })
                        );
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        {
            let mut handles = self.handles.lock().unwrap();
            handles.push(handle);
        }

        Ok(format!("倒计时已启动，时长 {} 分钟", minutes))
    }

    pub fn start_scheduled_timer(
        &self,
        app_handle: tauri::AppHandle,
        task_id: i64,
        target_timestamp: u64,
    ) -> Result<String, String> {
        let current_time = Self::get_current_timestamp();
        if target_timestamp <= current_time {
            return Err("目标时间必须大于当前时间".to_string());
        }

        self.stop_timer(task_id);

        let timer = Timer {
            task_id,
            timer_type: "scheduled".to_string(),
            target_time: target_timestamp,
            is_running: true,
        };

        {
            let mut timers = self.timers.lock().unwrap();
            timers.push(timer);
        }

        let app_handle_clone = app_handle.clone();
        let timers_clone = self.timers.clone();
        let task_id_clone = task_id;

        let handle = spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                let current_time = TimerManager::get_current_timestamp();
                let mut timers = timers_clone.lock().unwrap();

                if let Some(timer) = timers.iter_mut().find(|t| t.task_id == task_id_clone) {
                    if !timer.is_running {
                        break;
                    }

                    let remaining = if current_time >= timer.target_time {
                        0
                    } else {
                        timer.target_time - current_time
                    };

                    let hours = remaining / 3600;
                    let minutes = (remaining % 3600) / 60;
                    let seconds = remaining % 60;

                    let _ = app_handle_clone.emit(
                        "timer_update",
                        serde_json::json!({
                            "task_id": task_id_clone,
                            "remaining": remaining,
                            "hours": hours,
                            "minutes": minutes,
                            "seconds": seconds,
                            "formatted": format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                        })
                    );

                    if remaining == 0 {
                        timer.is_running = false;
                        let _ = app_handle_clone.emit(
                            "timer_expired",
                            serde_json::json!({
                                "task_id": task_id_clone,
                                "timerType": "scheduled"
                            })
                        );
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        {
            let mut handles = self.handles.lock().unwrap();
            handles.push(handle);
        }

        Ok("定时任务已启动".to_string())
    }

    pub fn stop_timer(&self, task_id: i64) {
        {
            let mut timers = self.timers.lock().unwrap();
            if let Some(timer) = timers.iter_mut().find(|t| t.task_id == task_id) {
                timer.is_running = false;
            }
            timers.retain(|t| t.task_id != task_id);
        }
        // 注意：tauri::async_runtime::JoinHandle 不提供 is_finished() 方法，
        // 任务通过 is_running 标志自行退出，无需显式清理 handles
    }

    pub fn get_timer_status(&self, task_id: i64) -> Option<TimerStatus> {
        let timers = self.timers.lock().unwrap();
        if let Some(timer) = timers.iter().find(|t| t.task_id == task_id) {
            let current_time = Self::get_current_timestamp();
            let remaining = if current_time >= timer.target_time {
                0
            } else {
                timer.target_time - current_time
            };

            let hours = remaining / 3600;
            let minutes = (remaining % 3600) / 60;
            let seconds = remaining % 60;

            Some(TimerStatus {
                task_id: timer.task_id,
                timer_type: timer.timer_type.clone(),
                remaining,
                hours,
                minutes,
                seconds,
                formatted: format!("{:02}:{:02}:{:02}", hours, minutes, seconds),
                is_running: timer.is_running,
            })
        } else {
            None
        }
    }

    pub fn calibrate_timer(&self, task_id: i64) -> Option<TimerStatus> {
        let mut timers = self.timers.lock().unwrap();
        if let Some(timer) = timers.iter_mut().find(|t| t.task_id == task_id) {
            let current_time = Self::get_current_timestamp();
            let remaining = if current_time >= timer.target_time {
                timer.is_running = false;
                0
            } else {
                timer.target_time - current_time
            };

            let hours = remaining / 3600;
            let minutes = (remaining % 3600) / 60;
            let seconds = remaining % 60;

            Some(TimerStatus {
                task_id: timer.task_id,
                timer_type: timer.timer_type.clone(),
                remaining,
                hours,
                minutes,
                seconds,
                formatted: format!("{:02}:{:02}:{:02}", hours, minutes, seconds),
                is_running: timer.is_running,
            })
        } else {
            None
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TimerStatus {
    pub task_id: i64,
    pub timer_type: String,
    pub remaining: u64,
    pub hours: u64,
    pub minutes: u64,
    pub seconds: u64,
    pub formatted: String,
    pub is_running: bool,
}

#[tauri::command]
pub fn start_countdown_cmd(
    app_handle: tauri::AppHandle,
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
    task_id: i64,
    minutes: i32,
) -> Result<String, String> {
    timer_manager.start_countdown(app_handle, task_id, minutes)
}

#[tauri::command]
pub fn start_scheduled_timer_cmd(
    app_handle: tauri::AppHandle,
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
    task_id: i64,
    target_timestamp: u64,
) -> Result<String, String> {
    timer_manager.start_scheduled_timer(app_handle, task_id, target_timestamp)
}

#[tauri::command]
pub fn stop_timer_cmd(
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
    task_id: i64,
) -> Result<String, String> {
    timer_manager.stop_timer(task_id);
    Ok("定时器已停止".to_string())
}

#[tauri::command]
pub fn get_timer_status_cmd(
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
    task_id: i64,
) -> Result<Option<TimerStatus>, String> {
    Ok(timer_manager.get_timer_status(task_id))
}

#[tauri::command]
pub fn calibrate_timer_cmd(
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
    task_id: i64,
) -> Result<Option<TimerStatus>, String> {
    Ok(timer_manager.calibrate_timer(task_id))
}