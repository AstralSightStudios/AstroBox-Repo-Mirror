//! 与手表端 SmsForwarder 应用同步（interconnect 桥接协议）
//!
//! 手表端应用使用极简桥接协议：
//! - 请求: `{ "type": "list_hosts", ...params }`
//! - 响应: `{ "type": "list_hosts_result", "hosts": [...] }` 或 `{ "data": [...] }`

use crate::astrobox::psys_host::{device, register, thirdpartyapp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

/// 手表端 SmsForwarder 应用包名（默认值，实际通过 resolve_target_pkg_name 动态解析）
const DEFAULT_PKG_NAME: &str = "com.whistleo.smsforwarder.client";
/// 目标应用名称关键词（用于在手表应用列表中查找）
const TARGET_APP_NAME_KEYWORD: &str = "SmsForwarder";

fn default_0() -> u32 { 0u32 }

/// 本地缓存中的短信转发主机条目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default = "default_0", rename = "encryptMode")]
    pub encrypt_mode: u32,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "sm4KeyHex")]
    pub sm4_key_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "createdAt")]
    pub created_at: Option<u64>,
}

/// 消息日志条目
#[derive(Debug, Clone)]
pub struct MessageLog {
    pub direction: String,
    pub timestamp: u64,
    pub msg_type: String,
    pub summary: String,
    pub status: String,
}

/// 同步状态
#[derive(Debug, Clone)]
pub struct SyncState {
    pub baseline_entries: Vec<HostEntry>,
    pub local_entries: Vec<HostEntry>,
    pub loaded: bool,
    pub status: String,
    pub last_device_addr: Option<String>,
    pub subscribed: bool,
    pub resolved_pkg_name: Option<String>,
    pub message_logs: Vec<MessageLog>,
    pub is_loading: bool,
    pub deleted_ids: Vec<String>,
}

static SYNC_STATE: OnceLock<Mutex<SyncState>> = OnceLock::new();

fn sync_state() -> &'static Mutex<SyncState> {
    SYNC_STATE.get_or_init(|| {
        Mutex::new(SyncState {
            baseline_entries: Vec::new(),
            local_entries: Vec::new(),
            loaded: false,
            status: "等待操作".to_string(),
            last_device_addr: None,
            subscribed: false,
            resolved_pkg_name: None,
            message_logs: Vec::new(),
            is_loading: false,
            deleted_ids: Vec::new(),
        })
    })
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn with_state<R>(f: impl FnOnce(&mut SyncState) -> R) -> R {
    let mut guard = sync_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

pub fn read_state<R>(f: impl FnOnce(&SyncState) -> R) -> R {
    let guard = sync_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&guard)
}

pub fn is_loading() -> bool {
    read_state(|state| state.is_loading)
}

pub fn set_loading(loading: bool) {
    with_state(|state| {
        state.is_loading = loading;
    });
}

fn log_message(direction: &str, msg_type: &str, summary: &str, status: &str) {
    with_state(|state| {
        state.message_logs.push(MessageLog {
            direction: direction.to_string(),
            timestamp: now_millis(),
            msg_type: msg_type.to_string(),
            summary: summary.to_string(),
            status: status.to_string(),
        });
        if state.message_logs.len() > 20 {
            state.message_logs.remove(0);
        }
    });
}

fn mark_sent_ok(msg_type: &str) {
    with_state(|state| {
        for log in state.message_logs.iter_mut().rev() {
            if log.direction == "sent" && log.msg_type == msg_type && log.status == "pending" {
                log.status = "ok".to_string();
                break;
            }
        }
    });
}

pub fn get_message_logs() -> Vec<MessageLog> {
    read_state(|state| state.message_logs.clone())
}

pub fn clear_message_logs() {
    with_state(|state| {
        state.message_logs.clear();
    });
}

pub fn handle_loading_timeout() {
    if is_loading() {
        tracing::warn!("loading timeout: resetting is_loading");
        set_loading(false);
        with_state(|state| {
            state.status = "加载超时，请重试".to_string();
        });
    }
}

#[derive(Debug, Clone)]
pub enum InterconnectResult {
    HostList(Vec<HostEntry>),
    OperationResult { message: String, is_error: bool },
}

pub async fn refresh_and_reregister() {
    let devices = device::get_connected_device_list().await;
    if devices.is_empty() {
        tracing::warn!("no connected devices, skipping re-register");
        return;
    }
    for dev in &devices {
        let pkg_name = resolve_target_pkg_name(&dev.addr)
            .await
            .unwrap_or_else(|_| DEFAULT_PKG_NAME.to_string());
        let _ = ensure_interconnect_registered(&dev.addr, &pkg_name).await;
    }
    with_state(|state| {
        state.subscribed = true;
        if let Some(dev) = devices.first() {
            state.last_device_addr = Some(dev.addr.clone());
        }
    });
    tracing::info!("refresh_and_reregister: done for {} devices", devices.len());
}

pub async fn first_connected_device() -> Result<device::DeviceInfo, String> {
    let devices = device::get_connected_device_list().await;
    devices
        .into_iter()
        .next()
        .ok_or_else(|| "未检测到已连接设备".to_string())
}

async fn resolve_target_pkg_name(addr: &str) -> Result<String, String> {
    let apps = thirdpartyapp::get_thirdparty_app_list(addr)
        .await
        .map_err(|e| format!("读取手表应用列表失败: {:?}", e))?;

    if let Some(app) = apps.iter().find(|a| a.app_name.contains(TARGET_APP_NAME_KEYWORD)) {
        tracing::info!("resolve_target_pkg_name: found by name '{}'", app.package_name);
        return Ok(app.package_name.clone());
    }

    if let Some(app) = apps.iter().find(|a| a.package_name == DEFAULT_PKG_NAME) {
        tracing::info!("resolve_target_pkg_name: found by pkg '{}'", app.package_name);
        return Ok(app.package_name.clone());
    }

    let app_names = apps
        .iter()
        .map(|a| format!("{}({})", a.app_name, a.package_name))
        .collect::<Vec<_>>()
        .join(", ");
    tracing::warn!("resolve_target_pkg_name: not found, apps={}", app_names);

    Err(format!(
        "手表未找到 SmsForwarder Client 应用。当前手表应用: {}",
        app_names
    ))
}

async fn ensure_interconnect_registered(addr: &str, pkg_name: &str) -> Result<(), String> {
    match register::register_interconnect_recv(addr, pkg_name).await {
        Ok(()) => {
            tracing::info!("interconnect registered: addr={}, pkg={}", addr, pkg_name);
        }
        Err(err) => {
            let raw = format!("{:?}", err);
            let lower = raw.to_lowercase();
            if lower.contains("already") || lower.contains("exists") || lower.contains("duplicate") {
                tracing::info!("interconnect already registered: addr={}, pkg={}", addr, pkg_name);
            } else {
                return Err(format!(
                    "注册接收失败(addr={}, pkg={}): {}",
                    addr, pkg_name, raw
                ));
            }
        }
    }
    Ok(())
}

pub async fn bootstrap_sync() -> Result<(bool, String), String> {
    let dev = first_connected_device().await?;

    let pkg_name = resolve_target_pkg_name(&dev.addr)
        .await
        .unwrap_or_else(|_| DEFAULT_PKG_NAME.to_string());

    ensure_interconnect_registered(&dev.addr, &pkg_name).await?;

    let status_msg = format!("已注册同步通道 ({}, {})", dev.addr, pkg_name);

    with_state(|state| {
        state.subscribed = true;
        state.last_device_addr = Some(dev.addr.clone());
        state.resolved_pkg_name = Some(pkg_name);
        state.status = status_msg.clone();
    });
    Ok((true, status_msg))
}

pub async fn bootstrap_if_needed() -> Result<(bool, String), String> {
    let already = read_state(|s| s.subscribed);
    if already {
        return Ok((true, "已注册".to_string()));
    }
    bootstrap_sync().await
}

async fn send_bridge_request(method: &str, params: Value) -> Result<(), String> {
    let device_addr = match crate::device::check_device().await {
        Some(addr) => addr,
        None => first_connected_device().await?.addr,
    };
    let pkg_name = match crate::device::resolve_pkg_name(&device_addr).await {
        Some(name) => name,
        None => resolve_target_pkg_name(&device_addr).await
            .unwrap_or_else(|_| DEFAULT_PKG_NAME.to_string()),
    };

    crate::device::ensure_registered(&device_addr, &pkg_name).await;

    with_state(|state| {
        state.subscribed = true;
        state.last_device_addr = Some(device_addr.clone());
        state.resolved_pkg_name = Some(pkg_name.clone());
    });

    let mut payload = serde_json::json!({ "type": method });
    if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            payload[k] = v.clone();
        }
    }

    let payload_str = payload.to_string();
    tracing::info!(
        "send_bridge_request: addr={}, pkg={}, payload={}",
        device_addr, pkg_name, &payload_str
    );

    let summary = &payload_str[..payload_str.len().min(80)];
    log_message("sent", method, summary, "pending");

    match crate::device::send_to_watch(&device_addr, &pkg_name, &payload_str).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let err_detail = format!("{:?}", e);
            tracing::error!(
                "send_bridge_request FAILED addr={}, pkg={}, err={}",
                device_addr, pkg_name, err_detail
            );
            let err_summary = format!("发送失败: {}", err_detail);
            let summary_cut = &err_summary[..err_summary.len().min(80)];
            log_message("sent", method, summary_cut, "error");
            Err(format!(
                "发送失败(addr={}, pkg={}): {}",
                device_addr, pkg_name, err_detail
            ))
        }
    }
}

pub async fn request_list_hosts() -> Result<(), String> {
    if is_loading() {
        return Err("正在从手表加载数据中，请稍候...".to_string());
    }
    set_loading(true);

    wit_bindgen::spawn(async move {
        let _ = crate::astrobox::psys_host::timer::set_timeout(10000, "loading_timeout").await;
    });

    let result = send_bridge_request("list_hosts", serde_json::json!({})).await;
    match result {
        Ok(()) => {
            with_state(|state| {
                state.status = "已请求主机列表，等待手表响应...".to_string();
            });
            Ok(())
        }
        Err(e) => {
            set_loading(false);
            Err(e)
        }
    }
}

pub async fn add_hosts_to_watch(entries: &[HostEntry]) -> Result<String, String> {
    if entries.is_empty() {
        return Err("没有可添加的条目".to_string());
    }

    let hosts: Vec<Value> = entries
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
        .collect();

    send_bridge_request("upsert_hosts", serde_json::json!({ "hosts": hosts }))
        .await?;

    let count = entries.len();
    with_state(|state| {
        state.status = format!("已发送 {} 条主机到手表，等待确认...", count);
    });

    Ok(format!("已发送 {} 条主机", count))
}

pub async fn send_sync_delta() -> Result<String, String> {
    let (upsert, remove) = get_sync_delta();

    if upsert.is_empty() && remove.is_empty() {
        return Err("没有增量数据需要同步".to_string());
    }

    let upsert_hosts: Vec<Value> = upsert
        .iter()
        .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
        .collect();

    let payload = serde_json::json!({
        "type": "sync_delta",
        "upsert": upsert_hosts,
        "remove": remove,
    });

    let payload_str = payload.to_string();
    tracing::info!("send_sync_delta: payload_len={}", payload_str.len());

    log_message("sent", "sync_delta", &format!("upsert={} remove={}", upsert.len(), remove.len()), "pending");

    let device_addr = match crate::device::check_device().await {
        Some(addr) => addr,
        None => first_connected_device().await?.addr,
    };
    let pkg_name = match crate::device::resolve_pkg_name(&device_addr).await {
        Some(name) => name,
        None => resolve_target_pkg_name(&device_addr).await
            .unwrap_or_else(|_| DEFAULT_PKG_NAME.to_string()),
    };

    crate::device::ensure_registered(&device_addr, &pkg_name).await;

    with_state(|state| {
        state.subscribed = true;
        state.last_device_addr = Some(device_addr.clone());
        state.resolved_pkg_name = Some(pkg_name.clone());
    });

    match crate::device::send_to_watch(&device_addr, &pkg_name, &payload_str).await {
        Ok(()) => {
            let count = upsert.len() + remove.len();
            with_state(|state| {
                state.status = format!("已发送 {} 条增量数据到手表，等待确认...", count);
            });
            Ok(format!("已发送 {} 条增量数据", count))
        }
        Err(e) => {
            let err_detail = format!("{:?}", e);
            tracing::error!("send_sync_delta FAILED: {}", err_detail);
            let err_summary = format!("发送失败: {}", err_detail);
            let summary_cut = &err_summary[..err_summary.len().min(80)];
            log_message("sent", "sync_delta", summary_cut, "error");
            Err(format!("发送增量同步失败: {}", err_detail))
        }
    }
}

fn extract_payload_text(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<Value>(payload) {
        if let Some(text) = json.get("payloadText").and_then(|v| v.as_str()) {
            return text.to_string();
        }
        if let Some(payload_value) = json.get("payload") {
            if let Some(text) = payload_value.as_str() {
                return text.to_string();
            }
            return payload_value.to_string();
        }
        if let Some(text) = json.get("data").and_then(|v| v.as_str()) {
            return text.to_string();
        }
    }
    payload.to_string()
}

fn list_hosts_result(parsed: &Value) -> Option<Vec<HostEntry>> {
    if let Some(hosts) = parsed.get("hosts").and_then(|v| v.as_array()) {
        let entries: Vec<HostEntry> = hosts
            .iter()
            .filter_map(|a| serde_json::from_value(a.clone()).ok())
            .collect();
        return Some(entries);
    }
    if let Some(accounts) = parsed.get("accounts").and_then(|v| v.as_array()) {
        let entries: Vec<HostEntry> = accounts
            .iter()
            .filter_map(|a| serde_json::from_value(a.clone()).ok())
            .collect();
        return Some(entries);
    }
    None
}

pub fn handle_interconnect_response(payload: &str) -> Option<InterconnectResult> {
    tracing::info!("handle_ic: raw_len={}", payload.len());

    let actual_payload = extract_payload_text(payload);
    let preview_len = actual_payload.len().min(300);
    tracing::info!("handle_ic: extracted={}", &actual_payload[..preview_len]);

    let parsed = match serde_json::from_str::<Value>(&actual_payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("handle_ic: parse failed: {}", e);
            with_state(|state| {
                if state.is_loading {
                    state.is_loading = false;
                    state.status = "接收数据解析失败".to_string();
                }
            });
            return None;
        }
    };

    if let Some(data_array) = parsed.get("data").and_then(|v| v.as_array()) {
        let entries: Vec<HostEntry> = data_array
            .iter()
            .filter_map(|a| serde_json::from_value(a.clone()).ok())
            .collect();
        let count = entries.len();
        tracing::info!("handle_ic: data format, {} entries", count);
        with_state(|state| {
            state.baseline_entries = entries.clone();
            state.local_entries = entries;
            state.loaded = true;
            state.is_loading = false;
            state.status = format!("已加载 {} 条主机", count);
        });
        return Some(InterconnectResult::HostList(read_state(|s| s.local_entries.clone())));
    }

    let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "bridge_status" => {
            let phase = parsed.get("phase").and_then(|v| v.as_str()).unwrap_or("unknown");
            let detail = parsed.get("detail").and_then(|v| v.as_str()).unwrap_or("");
            let message = if detail.is_empty() {
                format!("手表状态: {}", phase)
            } else {
                format!("手表状态: {} ({})", phase, detail)
            };
            log_message("received", "bridge_status", &format!("phase={}", phase), "ok");
            with_state(|state| { state.status = message.clone(); });
            Some(InterconnectResult::OperationResult { message, is_error: false })
        }
        "list_hosts_result" => {
            log_message("received", "list_hosts_result", "", "ok");
            mark_sent_ok("list_hosts");
            with_state(|state| { state.is_loading = false; });
            if let Some(entries) = list_hosts_result(&parsed) {
                let count = entries.len();
                tracing::info!("handle_ic: list_hosts_result format, {} entries", count);
                with_state(|state| {
                    state.baseline_entries = entries.clone();
                    state.local_entries = entries;
                    state.loaded = true;
                    state.status = format!("已加载 {} 条主机", count);
                });
                Some(InterconnectResult::HostList(read_state(|s| s.local_entries.clone())))
            } else if let Some(error) = parsed.get("error").and_then(|v| v.as_str()) {
                Some(InterconnectResult::OperationResult {
                    message: format!("手表返回错误: {}", error),
                    is_error: true,
                })
            } else {
                None
            }
        }
        "upsert_hosts_result" => {
            log_message("received", "upsert_hosts_result", "", "ok");
            mark_sent_ok("upsert_hosts");
            let message = if parsed.get("error").is_some() {
                format!("同步失败: {}", parsed.get("error").and_then(|v| v.as_str()).unwrap_or("未知错误"))
            } else {
                "同步成功".to_string()
            };
            Some(InterconnectResult::OperationResult { message, is_error: parsed.get("error").is_some() })
        }
        "remove_host_result" => {
            log_message("received", "remove_host_result", "", "ok");
            mark_sent_ok("remove_host");
            let message = if parsed.get("error").is_some() {
                format!("删除失败: {}", parsed.get("error").and_then(|v| v.as_str()).unwrap_or("未知错误"))
            } else {
                "删除成功".to_string()
            };
            Some(InterconnectResult::OperationResult { message, is_error: parsed.get("error").is_some() })
        }
        "sync_delta_result" => {
            log_message("received", "sync_delta_result", "", "ok");
            mark_sent_ok("sync_delta");
            let message = if parsed.get("error").is_some() {
                format!("增量同步失败: {}", parsed.get("error").and_then(|v| v.as_str()).unwrap_or("未知错误"))
            } else {
                "增量同步成功".to_string()
            };
            let is_error = parsed.get("error").is_some();
            if !is_error {
                clear_sync_delta();
            }
            Some(InterconnectResult::OperationResult { message, is_error })
        }
        "error" => {
            let error_msg = parsed.get("error").and_then(|v| v.as_str()).unwrap_or("未知错误");
            log_message("received", "error", error_msg, "error");
            with_state(|state| { state.is_loading = false; });
            Some(InterconnectResult::OperationResult {
                message: format!("手表返回错误: {}", error_msg),
                is_error: true,
            })
        }
        "echo" | "ack" => {
            log_message("received", msg_type, "", "ok");
            let head = parsed.get("head").and_then(|v| v.as_str()).unwrap_or("");
            let len = parsed.get("len").and_then(|v| v.as_u64()).unwrap_or(0);
            let received = parsed.get("received").and_then(|v| v.as_str()).unwrap_or("");
            let message = if !received.is_empty() {
                format!("手环已收到请求: {}", received)
            } else if !head.is_empty() {
                format!("手环回声: {}... ({}B)", head, len)
            } else {
                "手环通信正常".to_string()
            };
            Some(InterconnectResult::OperationResult { message, is_error: false })
        }
        _ => {
            log_message("received", "unknown", msg_type, "error");
            tracing::warn!("handle_ic: unknown type={}, keys={:?}", msg_type, parsed.as_object().map(|o| o.keys().collect::<Vec<_>>()));
            with_state(|state| { state.is_loading = false; });
            None
        }
    }
}

pub fn add_local_entry(entry: HostEntry) {
    with_state(|state| {
        state.local_entries.push(entry);
        state.status = "已添加到本地列表（需保存修改至手表生效）".to_string();
    });
}

pub fn delete_local_entry(index: usize) {
    with_state(|state| {
        if index < state.local_entries.len() {
            let entry = state.local_entries.remove(index);
            let id = entry.id.clone();
            if !state.deleted_ids.contains(&id) {
                state.deleted_ids.push(id);
            }
            state.status = "已删除本地条目（需保存修改至手表生效）".to_string();
        }
    });
}

pub fn update_local_entry_by_url(url: &str, entry: HostEntry) {
    with_state(|state| {
        if let Some(idx) = state.local_entries.iter().position(|e| e.url == url) {
            state.local_entries[idx] = entry;
            state.status = "已修改本地条目（需保存修改至手表生效）".to_string();
        }
    });
}

pub fn get_entry_by_index(index: usize) -> Option<HostEntry> {
    read_state(|state| state.local_entries.get(index).cloned())
}

pub fn get_sync_delta() -> (Vec<HostEntry>, Vec<String>) {
    read_state(|state| {
        let mut upsert = Vec::new();

        for entry in &state.local_entries {
            if let Some(base) = state.baseline_entries.iter().find(|b| b.id == entry.id) {
                if entry != base {
                    upsert.push(entry.clone());
                }
            } else {
                upsert.push(entry.clone());
            }
        }

        (upsert, state.deleted_ids.clone())
    })
}

pub fn clear_sync_delta() {
    with_state(|state| {
        state.deleted_ids.clear();
        state.baseline_entries = state.local_entries.clone();
    });
}

pub fn validate_form(e: &HostEntry) -> Result<(), String> {
    if e.name.trim().is_empty() {
        return Err("名称不能为空".to_string());
    }
    if e.url.trim().is_empty() {
        return Err("URL 不能为空".to_string());
    }
    match e.encrypt_mode {
        0 => {}
        1 => {
            if e.secret.trim().is_empty() {
                return Err("加密模式为对称加密时，密钥(secret)不能为空".to_string());
            }
        }
        3 => {
            let sm4_raw = e.sm4_key_hex.as_deref().unwrap_or("");
            let sm4: String = sm4_raw
                .chars()
                .filter(|c| !c.is_whitespace())
                .map(|c| c.to_ascii_uppercase())
                .collect();
            if sm4.len() != 32 {
                return Err(format!("SM4 密钥长度错误: 期望 32 个十六进制字符，实际 {}", sm4.len()));
            }
            for ch in sm4.chars() {
                if !ch.is_ascii_hexdigit() {
                    return Err("SM4 密钥包含非法字符: 仅允许 [0-9a-fA-F]".to_string());
                }
            }
        }
        other => {
            return Err(format!("未知的加密模式: {}", other));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        encrypt_mode: u32,
        secret: &str,
        sm4: Option<&str>,
    ) -> HostEntry {
        HostEntry {
            id: "h_1".to_string(),
            name: "test".to_string(),
            url: "https://example.com".to_string(),
            secret: secret.to_string(),
            encrypt_mode,
            sm4_key_hex: sm4.map(|s| s.to_string()),
            connected: None,
            created_at: None,
        }
    }

    #[test]
    fn validate_form_mode0_plain_ok() {
        let e = make_entry(0, "", None);
        assert!(validate_form(&e).is_ok());
    }

    #[test]
    fn validate_form_mode1_secret_ok() {
        let e = make_entry(1, "my_secret", None);
        assert!(validate_form(&e).is_ok());
    }

    #[test]
    fn validate_form_mode3_sm4_ok() {
        let key_hex = "0123456789abcdef0123456789ABCDEF";
        let e = make_entry(3, "", Some(key_hex));
        assert!(validate_form(&e).is_ok());
    }

    #[test]
    fn validate_form_name_empty_fail() {
        let mut e = make_entry(0, "", None);
        e.name = "".to_string();
        let err = validate_form(&e).expect_err("should fail");
        assert!(err.contains("名称"));
    }

    #[test]
    fn validate_form_mode1_secret_empty_fail() {
        let e = make_entry(1, "", None);
        let err = validate_form(&e).expect_err("should fail");
        assert!(err.contains("secret") || err.contains("密钥"));
    }

    #[test]
    fn validate_form_mode3_sm4_wrong_length_fail() {
        let e = make_entry(3, "", Some("abcd"));
        let err = validate_form(&e).expect_err("should fail");
        assert!(err.contains("长度"));
    }

    #[test]
    fn handle_response_data_array_format() {
        with_state(|state| {
            state.baseline_entries.clear();
            state.local_entries.clear();
            state.loaded = false;
            state.is_loading = true;
        });

        let payload = serde_json::json!({
            "data": [
                {
                    "id": "h_a",
                    "name": "Alpha",
                    "url": "https://a.example.com",
                    "encryptMode": 0
                },
                {
                    "id": "h_b",
                    "name": "Beta",
                    "url": "https://b.example.com",
                    "encryptMode": 1,
                    "secret": "s"
                }
            ]
        }).to_string();

        let result = handle_interconnect_response(&payload);
        match result {
            Some(InterconnectResult::HostList(list)) => {
                assert_eq!(list.len(), 2);
                assert_eq!(list[0].name, "Alpha");
                assert_eq!(list[1].name, "Beta");
            }
            other => panic!("expected HostList, got {:?}", other),
        }

        assert!(read_state(|s| s.loaded));
        assert!(!read_state(|s| s.is_loading));
        assert_eq!(read_state(|s| s.local_entries.len()), 2);
        assert_eq!(read_state(|s| s.baseline_entries.len()), 2);
    }

    #[test]
    fn handle_response_payload_text_wrapped_list_hosts_result() {
        with_state(|state| {
            state.baseline_entries.clear();
            state.local_entries.clear();
            state.loaded = false;
            state.is_loading = true;
        });

        let inner = serde_json::json!({
            "type": "list_hosts_result",
            "hosts": [
                {
                    "id": "h_x",
                    "name": "Imported",
                    "url": "https://x.example.com",
                    "encryptMode": 0
                }
            ]
        }).to_string();

        let payload = serde_json::json!({
            "payloadText": inner
        }).to_string();

        let result = handle_interconnect_response(&payload);
        match result {
            Some(InterconnectResult::HostList(list)) => {
                assert_eq!(list.len(), 1);
                assert_eq!(list[0].id, "h_x");
                assert_eq!(list[0].name, "Imported");
            }
            other => panic!("expected HostList, got {:?}", other),
        }

        assert!(read_state(|s| s.loaded));
        assert!(!read_state(|s| s.is_loading));
        assert_eq!(read_state(|s| s.local_entries.len()), 1);
        assert_eq!(read_state(|s| s.baseline_entries.len()), 1);
    }
}
