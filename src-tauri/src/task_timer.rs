use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::async_runtime::spawn;
use tokio::time::interval;
use tauri::Emitter;
use winapi::shared::minwindef::{BOOL, UINT};
use libloading::{Library, Symbol};

pub struct TimerManager {
    timers: Arc<Mutex<HashMap<i64, Timer>>>,
    is_alarm_playing: Arc<AtomicBool>,
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
            timers: Arc::new(Mutex::new(HashMap::new())),
            is_alarm_playing: Arc::new(AtomicBool::new(false)),
        }
    }

    fn get_current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn spawn_timer_task(
        app_handle: tauri::AppHandle,
        timers: Arc<Mutex<HashMap<i64, Timer>>>,
        task_id: i64,
        timer_type: String,
    ) {
        spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                let current_time = Self::get_current_timestamp();
                let mut timers_lock = timers.lock().unwrap();

                let (remaining, is_running) = timers_lock
                    .get_mut(&task_id)
                    .map(|t| {
                        let remaining = if current_time >= t.target_time {
                            0
                        } else {
                            t.target_time - current_time
                        };
                        (remaining, t.is_running)
                    })
                    .unwrap_or((0, false));

                if !is_running {
                    break;
                }

                let hours = remaining / 3600;
                let minutes = (remaining % 3600) / 60;
                let seconds = remaining % 60;

                let _ = app_handle.emit(
                    "timer_update",
                    serde_json::json!({
                        "task_id": task_id,
                        "remaining": remaining,
                        "hours": hours,
                        "minutes": minutes,
                        "seconds": seconds,
                        "formatted": format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
                    })
                );

                if remaining == 0 {
                    if let Some(timer) = timers_lock.get_mut(&task_id) {
                        timer.is_running = false;
                    }
                    let _ = app_handle.emit(
                        "timer_expired",
                        serde_json::json!({
                            "task_id": task_id,
                            "timerType": timer_type
                        })
                    );
                    break;
                }
            }
        });
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
            timers.insert(task_id, timer);
        }

        Self::spawn_timer_task(
            app_handle,
            self.timers.clone(),
            task_id,
            "countdown".to_string(),
        );

        Ok(format!("{}", target_time))
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
            timers.insert(task_id, timer);
        }

        Self::spawn_timer_task(
            app_handle,
            self.timers.clone(),
            task_id,
            "scheduled".to_string(),
        );

        Ok("定时任务已启动".to_string())
    }

    pub fn restore_scheduled_timer(
        &self,
        app_handle: tauri::AppHandle,
        task_id: i64,
        target_timestamp: u64,
    ) -> Result<String, String> {
        let current_time = Self::get_current_timestamp();
        if target_timestamp <= current_time {
            return Err("目标时间已过期".to_string());
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
            timers.insert(task_id, timer);
        }

        Self::spawn_timer_task(
            app_handle,
            self.timers.clone(),
            task_id,
            "scheduled".to_string(),
        );

        Ok("定时任务已恢复".to_string())
    }

    pub fn stop_timer(&self, task_id: i64) {
        {
            let mut timers = self.timers.lock().unwrap();
            if let Some(timer) = timers.get_mut(&task_id) {
                timer.is_running = false;
            }
            timers.remove(&task_id);
        }
    }

    pub fn stop_all_timers(&self) {
        let mut timers = self.timers.lock().unwrap();
        for (_, timer) in timers.iter_mut() {
            timer.is_running = false;
        }
        timers.clear();
    }

    pub fn stop_alarm(&self) {
        self.is_alarm_playing.store(false, Ordering::SeqCst);
        let _ = play_sound(None, 0);
    }

    pub fn get_timer_status(&self, task_id: i64) -> Option<TimerStatus> {
        let timers = self.timers.lock().unwrap();
        if let Some(timer) = timers.get(&task_id) {
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
        if let Some(timer) = timers.get_mut(&task_id) {
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
pub fn restore_scheduled_timer_cmd(
    app_handle: tauri::AppHandle,
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
    task_id: i64,
    target_timestamp: u64,
) -> Result<String, String> {
    timer_manager.restore_scheduled_timer(app_handle, task_id, target_timestamp)
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

use std::sync::OnceLock;

static WINMM_LIB: OnceLock<Library> = OnceLock::new();

fn get_winmm_lib() -> Option<&'static Library> {
    WINMM_LIB.get_or_init(|| unsafe { Library::new("winmm.dll").unwrap_or_else(|_| {
        panic!("Failed to load winmm.dll")
    })});
    WINMM_LIB.get()
}

type PlaySoundWFunc = unsafe extern "system" fn(
    psz_sound: *const u16,
    hmod: *const std::ffi::c_void,
    fdw_sound: UINT,
) -> BOOL;

fn play_sound(sound_path: Option<&str>, flags: UINT) -> bool {
    let lib = match get_winmm_lib() {
        Some(l) => l,
        None => return false,
    };
    
    unsafe {
        let play_sound: Symbol<PlaySoundWFunc> = match lib.get(b"PlaySoundW\0") {
            Ok(f) => f,
            Err(_) => return false,
        };
        
        match sound_path {
            Some(path) => {
                let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                play_sound(wide_path.as_ptr(), std::ptr::null(), flags) != 0
            }
            None => {
                play_sound(std::ptr::null(), std::ptr::null(), flags) != 0
            }
        }
    }
}

#[tauri::command]
pub fn play_alarm_cmd(
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
) -> Result<String, String> {
    let is_alarm_playing = timer_manager.is_alarm_playing.clone();
    
    if is_alarm_playing.load(Ordering::SeqCst) {
        return Ok("闹钟已在播放中".to_string());
    }

    is_alarm_playing.store(true, Ordering::SeqCst);

    let alarm_sound = "C:\\Windows\\Media\\Alarm01.wav";
    let snd_async: UINT = 0x0001;
    let snd_loop: UINT = 0x0008;
    let snd_nostop: UINT = 0x0010;

    let success = play_sound(Some(alarm_sound), snd_async | snd_loop | snd_nostop);
    
    if !success {
        is_alarm_playing.store(false, Ordering::SeqCst);
        return Ok("播放闹钟失败".to_string());
    }

    spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        if is_alarm_playing.load(Ordering::SeqCst) {
            is_alarm_playing.store(false, Ordering::SeqCst);
            let _ = play_sound(None, 0);
        }
    });

    Ok("闹钟已启动，将播放15秒".to_string())
}

#[tauri::command]
pub fn stop_alarm_cmd(
    timer_manager: tauri::State<'_, Arc<TimerManager>>,
) -> Result<String, String> {
    timer_manager.is_alarm_playing.store(false, Ordering::SeqCst);
    
    let _ = play_sound(None, 0);

    Ok("闹钟已停止".to_string())
}