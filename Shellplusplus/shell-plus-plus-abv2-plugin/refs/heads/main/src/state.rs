use std::sync::{Mutex, OnceLock};

use crate::protocol::ScreenshotItem;

#[derive(Debug, Clone, Default)]
pub struct DeviceSummary {
    pub name: String,
    pub addr: String,
}

#[derive(Debug, Clone)]
pub struct ScreenshotTransfer {
    pub source_session_id: String,
    pub save_session_id: Option<u64>,
    pub shot_id: String,
    pub file_name: String,
    pub total: i64,
    pub received: i64,
    pub received_bytes: i64,
    pub size: i64,
    pub platform: String,
    pub mode_label: String,
    pub started_at_ms: u128,
    pub rate_kbps: f64,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub root_element_id: Option<String>,
    pub active_panel: String,
    pub cli_enabled: bool,
    pub cli_registered: bool,
    pub selected_device: Option<DeviceSummary>,
    pub target_pkg_name: String,
    pub screenshots: Vec<ScreenshotItem>,
    pub selecting_screenshots: bool,
    pub selected_shot_ids: Vec<String>,
    pub sync_mode: String,
    pub fetch_url: String,
    pub terminal_status: String,
    pub terminal_input: String,
    pub terminal_output: String,
    pub terminal_last_command: String,
    pub pending_exec_req_id: String,
    pub pending_cli_callback: String,
    pub sync_queue: Vec<ScreenshotItem>,
    pub sync_total: usize,
    pub sync_done: usize,
    pub sync_failed: usize,
    pub active_transfer: Option<ScreenshotTransfer>,
    pub host_platform: String,
    pub connected: bool,
    pub registered_recv: bool,
    pub last_status: String,
    pub last_message: String,
    pub last_ui_event_id: String,
    pub last_ui_event_at_ms: u128,
    pub logs: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            root_element_id: None,
            active_panel: "device".to_string(),
            cli_enabled: true,
            cli_registered: false,
            selected_device: None,
            target_pkg_name: crate::protocol::QUICK_APP_PACKAGE.to_string(),
            screenshots: Vec::new(),
            selecting_screenshots: false,
            selected_shot_ids: Vec::new(),
            sync_mode: "interconnect".to_string(),
            fetch_url: String::new(),
            terminal_status: "等待输入命令".to_string(),
            terminal_input: String::new(),
            terminal_output: "暂无输出".to_string(),
            terminal_last_command: String::new(),
            pending_exec_req_id: String::new(),
            pending_cli_callback: String::new(),
            sync_queue: Vec::new(),
            sync_total: 0,
            sync_done: 0,
            sync_failed: 0,
            active_transfer: None,
            host_platform: String::new(),
            connected: false,
            registered_recv: false,
            last_status: "等待刷新设备".to_string(),
            last_message: String::new(),
            last_ui_event_id: String::new(),
            last_ui_event_at_ms: 0,
            logs: Vec::new(),
        }
    }
}

static STATE: OnceLock<Mutex<AppState>> = OnceLock::new();

pub fn with_state<R>(f: impl FnOnce(&mut AppState) -> R) -> R {
    let mutex = STATE.get_or_init(|| Mutex::new(AppState::default()));
    let mut guard = mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

pub fn snapshot() -> AppState {
    with_state(|state| state.clone())
}

pub fn append_log(message: impl Into<String>) {
    let message = message.into();
    crate::logger::info(&message);
    with_state(|state| {
        state.logs.push(message);
        if state.logs.len() > 80 {
            let overflow = state.logs.len() - 80;
            state.logs.drain(0..overflow);
        }
    });
}

pub fn append_warn(message: impl Into<String>) {
    let message = message.into();
    crate::logger::warn(&message);
    with_state(|state| {
        state.logs.push(format!("WARN {}", message));
        if state.logs.len() > 80 {
            let overflow = state.logs.len() - 80;
            state.logs.drain(0..overflow);
        }
    });
}
