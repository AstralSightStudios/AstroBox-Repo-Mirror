use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::ui::persist;

/// 主Tab枚举
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Connect,
    About,
}

/// 应用连接状态
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AppConnectionStatus {
    /// 未连接（灰色圆点，显示[连接]）
    #[default]
    Disconnected,
    /// 握手中（黄色圆点，显示禁用的[连接中…]）
    Handshaking,
    /// 已连接（绿色圆点，显示[断开]并展开详细信息）
    Connected,
    /// 连接失败（红色圆点，显示[重试]和失败原因）
    Failed,
}

/// 设备上安装的第三方快应用
#[derive(Clone, Debug)]
pub struct InstalledApp {
    pub addr: String,
    pub device_name: String,
    pub package_name: String,
    pub app_name: String,
    pub version_code: u32,
}

/// 单个应用的运行时连接状态与统计（按 (设备地址, 包名) 索引）
#[derive(Clone, Debug, Default)]
pub struct AppConnection {
    pub status: AppConnectionStatus,
    pub request_count: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub last_status: Option<String>,
    pub last_url: Option<String>,
    /// 连接失败时的原因
    pub fail_reason: Option<String>,
}

/// 持久化到磁盘的应用统计条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedApp {
    pub pkg_name: String,
    #[serde(default)]
    pub request_count: u64,
    #[serde(default)]
    pub success_count: u64,
    #[serde(default)]
    pub error_count: u64,
}

/// 插件全局状态
pub struct PluginState {
    pub current_tab: MainTab,
    pub root_element_id: Option<String>,

    /// 已连接设备 (addr, name)
    pub connected_devices: Vec<(String, String)>,
    /// 所有设备上的第三方快应用
    pub installed_apps: Vec<InstalledApp>,

    /// 按 (设备地址, 包名) 索引的连接状态与统计
    pub connections: HashMap<(String, String), AppConnection>,

    /// 待确认的握手：(设备地址, 包名) -> 定时器ID
    pub pending_handshakes: HashMap<(String, String), u64>,

    /// 心跳检测间隔定时器ID
    pub heartbeat_timer: Option<u64>,
    /// 每个(设备,包名)最近一次收到 SF_PING 的时间戳(ms)
    pub last_ping_ms: HashMap<(String, String), u128>,

    /// 上次自动刷新时间戳（节流用）
    pub last_auto_refresh_ms: u128,

    pub devices_loading: bool,
    pub apps_loading: bool,
    pub render_tick: u64,
}

static STATE: OnceLock<Mutex<PluginState>> = OnceLock::new();

pub fn with_state<R>(f: impl FnOnce(&mut PluginState) -> R) -> R {
    let mutex = STATE.get_or_init(|| {
        Mutex::new(PluginState {
            current_tab: MainTab::Connect,
            root_element_id: None,
            connected_devices: Vec::new(),
            installed_apps: Vec::new(),
            connections: HashMap::new(),
            pending_handshakes: HashMap::new(),
            heartbeat_timer: None,
            last_ping_ms: HashMap::new(),
            last_auto_refresh_ms: 0,
            devices_loading: false,
            apps_loading: false,
            render_tick: 0,
        })
    });
    let mut guard = mutex.lock().unwrap_or_else(|p| p.into_inner());
    f(&mut guard)
}

#[inline]
fn key(addr: &str, pkg: &str) -> (String, String) {
    (addr.to_string(), pkg.to_string())
}

// ========== 连接状态访问 ==========

pub fn connection_status(addr: &str, pkg: &str) -> AppConnectionStatus {
    with_state(|s| {
        s.connections
            .get(&key(addr, pkg))
            .map(|c| c.status)
            .unwrap_or(AppConnectionStatus::Disconnected)
    })
}

pub fn connection(addr: &str, pkg: &str) -> Option<AppConnection> {
    with_state(|s| s.connections.get(&key(addr, pkg)).cloned())
}

pub fn set_connection_status(addr: &str, pkg: &str, status: AppConnectionStatus) {
    with_state(|s| {
        let entry = s.connections.entry(key(addr, pkg)).or_default();
        entry.status = status;
        if status != AppConnectionStatus::Failed {
            entry.fail_reason = None;
        }
        // 连接建立时记录初始心跳时间；断开时清除
        if status == AppConnectionStatus::Connected {
            s.last_ping_ms.insert(key(addr, pkg), now_unix_ms());
        } else if status == AppConnectionStatus::Disconnected {
            s.last_ping_ms.remove(&key(addr, pkg));
        }
        s.render_tick = s.render_tick.wrapping_add(1);
    });
}

pub fn set_connection_failed(addr: &str, pkg: &str, reason: &str) {
    with_state(|s| {
        let entry = s.connections.entry(key(addr, pkg)).or_default();
        entry.status = AppConnectionStatus::Failed;
        entry.fail_reason = Some(reason.to_string());
        s.render_tick = s.render_tick.wrapping_add(1);
    });
}

pub fn record_request(addr: &str, pkg: &str, url: Option<&str>) {
    with_state(|s| {
        let entry = s.connections.entry(key(addr, pkg)).or_default();
        entry.request_count = entry.request_count.saturating_add(1);
        if let Some(u) = url {
            entry.last_url = Some(u.to_string());
        }
        s.render_tick = s.render_tick.wrapping_add(1);
    });
    persist_now();
}

pub fn record_result(addr: &str, pkg: &str, ok: bool, status: Option<String>) {
    with_state(|s| {
        let entry = s.connections.entry(key(addr, pkg)).or_default();
        if ok {
            entry.success_count = entry.success_count.saturating_add(1);
        } else {
            entry.error_count = entry.error_count.saturating_add(1);
        }
        if let Some(st) = status {
            entry.last_status = Some(st);
        }
        s.render_tick = s.render_tick.wrapping_add(1);
    });
    persist_now();
}

// ========== 握手管理 ==========

pub fn insert_pending_handshake(addr: &str, pkg: &str, timer_id: u64) -> Option<u64> {
    with_state(|s| s.pending_handshakes.insert(key(addr, pkg), timer_id))
}

pub fn take_pending_handshake(addr: &str, pkg: &str) -> Option<u64> {
    with_state(|s| s.pending_handshakes.remove(&key(addr, pkg)))
}

pub fn is_handshake_pending(addr: &str, pkg: &str) -> bool {
    with_state(|s| s.pending_handshakes.contains_key(&key(addr, pkg)))
}

pub fn drain_pending_for_device(addr: &str) -> Vec<u64> {
    with_state(|s| {
        let keys: Vec<(String, String)> = s
            .pending_handshakes
            .keys()
            .filter(|(a, _)| a == addr)
            .cloned()
            .collect();
        keys.iter()
            .filter_map(|k| s.pending_handshakes.remove(k))
            .collect()
    })
}

// ========== 设备/应用列表 ==========

pub fn set_root(element_id: &str) {
    with_state(|s| s.root_element_id = Some(element_id.to_string()));
}

pub fn root() -> Option<String> {
    with_state(|s| s.root_element_id.clone())
}

pub fn set_connected_devices(devices: Vec<(String, String)>) {
    with_state(|s| {
        s.connected_devices = devices;
        s.devices_loading = false;
        s.render_tick = s.render_tick.wrapping_add(1);
    });
}

pub fn connected_devices() -> Vec<(String, String)> {
    with_state(|s| s.connected_devices.clone())
}

pub fn set_installed_apps(apps: Vec<InstalledApp>) {
    with_state(|s| {
        s.installed_apps = apps;
        s.apps_loading = false;
        s.render_tick = s.render_tick.wrapping_add(1);
    });
}

pub fn installed_apps() -> Vec<InstalledApp> {
    with_state(|s| s.installed_apps.clone())
}

pub fn set_devices_loading(loading: bool) {
    with_state(|s| s.devices_loading = loading);
}

pub fn set_apps_loading(loading: bool) {
    with_state(|s| s.apps_loading = loading);
}

pub fn is_devices_loading() -> bool {
    with_state(|s| s.devices_loading)
}

pub fn is_apps_loading() -> bool {
    with_state(|s| s.apps_loading)
}

/// 清理已不在 installed_apps 中的连接
pub fn prune_stale_connections() {
    with_state(|s| {
        let valid: std::collections::HashSet<(String, String)> = s
            .installed_apps
            .iter()
            .map(|a| (a.addr.clone(), a.package_name.clone()))
            .collect();
        s.connections.retain(|k, _| valid.contains(k));
        s.pending_handshakes.retain(|k, _| valid.contains(k));
    });
}

// ========== 心跳追踪 ==========

/// 记录收到心跳的时间
pub fn record_ping(addr: &str, pkg: &str) {
    with_state(|s| {
        s.last_ping_ms.insert(key(addr, pkg), now_unix_ms());
    });
}

/// 返回所有已连接但超过 threshold_ms 未收到心跳的 (addr, pkg)
pub fn stale_connections(threshold_ms: u128) -> Vec<(String, String)> {
    let now = now_unix_ms();
    with_state(|s| {
        s.connections
            .iter()
            .filter(|(_, c)| c.status == AppConnectionStatus::Connected)
            .filter_map(|(k, _)| {
                let last = s.last_ping_ms.get(k).copied().unwrap_or(0);
                if now.saturating_sub(last) > threshold_ms {
                    Some(k.clone())
                } else {
                    None
                }
            })
            .collect()
    })
}

pub fn set_heartbeat_timer(id: u64) {
    with_state(|s| s.heartbeat_timer = Some(id));
}

/// 节流自动刷新：返回 true 表示允许执行一次刷新
pub fn try_claim_auto_refresh(min_interval_ms: u128) -> bool {
    let now = now_unix_ms();
    with_state(|s| {
        if now.saturating_sub(s.last_auto_refresh_ms) < min_interval_ms {
            return false;
        }
        s.last_auto_refresh_ms = now;
        true
    })
}

pub fn now_unix_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ========== 持久化 ==========

/// 从磁盘恢复统计数据（启动时不再自动连接）
pub fn restore_from_disk(_config: persist::OnDiskConfig) {
    // 自动重连已移除；目前仅保留配置文件读取以兼容旧配置，
    // 运行时统计在会话内重新累计。
}

/// 生成待持久化的应用列表（有统计的应用，按包名聚合）
fn snapshot_persisted_apps(s: &PluginState) -> Vec<PersistedApp> {
    let mut by_pkg: HashMap<String, PersistedApp> = HashMap::new();
    for ((_addr, pkg), conn) in &s.connections {
        let entry = by_pkg.entry(pkg.clone()).or_insert(PersistedApp {
            pkg_name: pkg.clone(),
            request_count: 0,
            success_count: 0,
            error_count: 0,
        });
        entry.request_count += conn.request_count;
        entry.success_count += conn.success_count;
        entry.error_count += conn.error_count;
    }
    by_pkg.into_values().collect()
}

pub fn persist_now() {
    let config = with_state(|s| {
        let apps = snapshot_persisted_apps(s);
        persist::OnDiskConfig {
            version: persist::CURRENT_VERSION,
            apps,
        }
    });
    persist::save_config(&config);
}
