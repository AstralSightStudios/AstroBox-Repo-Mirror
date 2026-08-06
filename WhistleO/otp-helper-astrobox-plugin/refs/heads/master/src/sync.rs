//! 与手表端认证器应用同步（interconnect 桥接协议）
//!
//! 手表端应用使用 JSON-RPC 风格的桥接协议：
//! - 请求: `{ "id": "xxx", "method": "list_accounts", "params": {...} }`
//! - 响应: `{ "id": "xxx", "result": {...} }` 或 `{ "id": "xxx", "error": "..." }`

use crate::astrobox::psys_host::{device, interconnect, register, thirdpartyapp};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// 手表端认证器应用包名（默认值，实际通过 resolve_target_pkg_name 动态解析）
const DEFAULT_AUTH_PKG_NAME: &str = "com.whistleo.otp";
/// 目标应用名称关键词（用于在手表应用列表中查找）
const TARGET_APP_NAME_KEYWORD: &str = "OTP";

/// 本地缓存中的 TOTP 条目
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TotpEntry {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    pub secret: String,
    #[serde(default = "default_type", rename = "type")]
    pub otp_type: String,
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    #[serde(default = "default_digits")]
    pub digits: u32,
    #[serde(default = "default_period")]
    pub period: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "extraParams")]
    pub extra_params: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "createdAt")]
    pub created_at: Option<u64>,
}

fn default_type() -> String { "totp".to_string() }
fn default_algorithm() -> String { "SHA1".to_string() }
fn default_digits() -> u32 { 6 }
fn default_period() -> u32 { 30 }

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
    /// 从手表加载的原始条目（基准）
    pub baseline_entries: Vec<TotpEntry>,
    /// 本地编辑后的条目列表
    pub local_entries: Vec<TotpEntry>,
    /// 是否已从手表加载
    pub loaded: bool,
    /// 状态消息
    pub status: String,
    /// 最后连接的设备地址
    pub last_device_addr: Option<String>,
    /// 是否已注册 interconnect 接收通道
    pub subscribed: bool,
    /// 动态解析的手表端包名
    pub resolved_pkg_name: Option<String>,
    /// 通信消息日志（最多保留 20 条）
    pub message_logs: Vec<MessageLog>,
    /// 是否正在从手表加载数据
    pub is_loading: bool,
    /// 已删除但尚未同步到手表的账号ID列表
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

/// 记录消息日志
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

/// 更新待处理请求的状态为 ok
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

/// 获取消息日志副本
pub fn get_message_logs() -> Vec<MessageLog> {
    read_state(|state| state.message_logs.clone())
}

/// 清除消息日志
pub fn clear_message_logs() {
    with_state(|state| {
        state.message_logs.clear();
    });
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

/// 检查是否正在从手表加载数据
pub fn is_loading() -> bool {
    read_state(|state| state.is_loading)
}

/// 设置加载状态
pub fn set_loading(loading: bool) {
    with_state(|state| {
        state.is_loading = loading;
    });
}

/// 加载超时处理：若超过指定时间仍未收到响应，自动重置加载状态
pub fn handle_loading_timeout() {
    if is_loading() {
        tracing::warn!("loading timeout: resetting is_loading");
        set_loading(false);
        with_state(|state| {
            state.status = "加载超时，请重试".to_string();
        });
    }
}

/// interconnect 响应结果
#[derive(Debug, Clone)]
pub enum InterconnectResult {
    /// 获取到的账号列表
    AccountList(Vec<TotpEntry>),
    /// 操作结果
    OperationResult { message: String, is_error: bool },
}

/// 设备连接/断开时重新注册 interconnect 接收通道（参考官方 InterconnectFetch 实现）
pub async fn refresh_and_reregister() {
    let devices = device::get_connected_device_list().await;
    if devices.is_empty() {
        tracing::warn!("no connected devices, skipping re-register");
        return;
    }
    for dev in &devices {
        let pkg_name = resolve_target_pkg_name(&dev.addr)
            .await
            .unwrap_or_else(|_| DEFAULT_AUTH_PKG_NAME.to_string());
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

/// 获取首个已连接设备
pub async fn first_connected_device() -> Result<device::DeviceInfo, String> {
    let devices = device::get_connected_device_list().await;
    devices
        .into_iter()
        .next()
        .ok_or_else(|| "未检测到已连接设备".to_string())
}

/// 从手表应用列表中解析目标快应用包名（参考 Varclass resolve_target_pkg_name）
async fn resolve_target_pkg_name(addr: &str) -> Result<String, String> {
    let apps = thirdpartyapp::get_thirdparty_app_list(addr)
        .await
        .map_err(|e| format!("读取手表应用列表失败: {:?}", e))?;

    // 优先按应用名关键词匹配
    if let Some(app) = apps.iter().find(|a| a.app_name.contains(TARGET_APP_NAME_KEYWORD)) {
        tracing::info!("resolve_target_pkg_name: found by name '{}'", app.package_name);
        return Ok(app.package_name.clone());
    }

    // 其次按默认包名匹配
    if let Some(app) = apps.iter().find(|a| a.package_name == DEFAULT_AUTH_PKG_NAME) {
        tracing::info!("resolve_target_pkg_name: found by pkg '{}'", app.package_name);
        return Ok(app.package_name.clone());
    }

    // 未找到，列出所有应用以便调试
    let app_names = apps
        .iter()
        .map(|a| format!("{}({})", a.app_name, a.package_name))
        .collect::<Vec<_>>()
        .join(", ");
    tracing::warn!("resolve_target_pkg_name: not found, apps={}", app_names);

    Err(format!(
        "手表未找到认证器应用。当前手表应用: {}",
        app_names
    ))
}

/// 注册 interconnect 接收通道（参考 Varclass：只注册手表端包名）
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

/// 启动时注册 interconnect 通道
pub async fn bootstrap_sync() -> Result<(bool, String), String> {
    let dev = first_connected_device().await?;

    let pkg_name = resolve_target_pkg_name(&dev.addr)
        .await
        .unwrap_or_else(|_| DEFAULT_AUTH_PKG_NAME.to_string());

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

/// 若未注册则自动注册
pub async fn bootstrap_if_needed() -> Result<(bool, String), String> {
    let already = read_state(|s| s.subscribed);
    if already {
        return Ok((true, "已注册".to_string()));
    }
    bootstrap_sync().await
}

/// 发送桥接请求到手表端认证器应用（参考 Varclass：极简 type 字段格式）
async fn send_bridge_request(method: &str, params: serde_json::Value) -> Result<(), String> {
    // 优先使用 device 模块的缓存，避免每次重复查询设备列表
    let device_addr = match crate::device::check_device().await {
        Some(addr) => addr,
        None => first_connected_device().await?.addr,
    };
    let pkg_name = match crate::device::resolve_pkg_name(&device_addr).await {
        Some(name) => name,
        None => resolve_target_pkg_name(&device_addr).await
            .unwrap_or_else(|_| DEFAULT_AUTH_PKG_NAME.to_string()),
    };

    crate::device::ensure_registered(&device_addr, &pkg_name).await;

    with_state(|state| {
        state.subscribed = true;
        state.last_device_addr = Some(device_addr.clone());
        state.resolved_pkg_name = Some(pkg_name.clone());
    });

    // 极简格式：{ "type": "list_accounts", ...params }
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

    log_message("sent", method, &payload_str[..payload_str.len().min(80)], "pending");

    match crate::device::send_to_watch(&device_addr, &pkg_name, &payload_str).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let err_detail = format!("{:?}", e);
            tracing::error!(
                "send_bridge_request FAILED addr={}, pkg={}, err={}",
                device_addr, pkg_name, err_detail
            );
            log_message("sent", method, &format!("发送失败: {}", err_detail)[..80], "error");
            Err(format!(
                "发送失败(addr={}, pkg={}): {}",
                device_addr, pkg_name, err_detail
            ))
        }
    }
}

/// 请求从手表加载所有账号
pub async fn request_list_accounts() -> Result<(), String> {
    if is_loading() {
        return Err("正在从手表加载数据中，请稍候...".to_string());
    }
    set_loading(true);

    // 启动 10 秒超时回退，防止响应丢失导致永久阻塞
    wit_bindgen::spawn(async move {
        let _ = crate::astrobox::psys_host::timer::set_timeout(10000, "loading_timeout").await;
    });

    let result = send_bridge_request("list_accounts", serde_json::json!({})).await;
    match result {
        Ok(()) => {
            with_state(|state| {
                state.status = "已请求账号列表，等待手表响应...".to_string();
            });
            Ok(())
        }
        Err(e) => {
            set_loading(false);
            Err(e)
        }
    }
}

/// 批量添加账号到手表（单向添加）
pub async fn add_accounts_to_watch(entries: &[TotpEntry]) -> Result<String, String> {
    if entries.is_empty() {
        return Err("没有可添加的条目".to_string());
    }

    let accounts: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| entry_to_account_value(e))
        .collect();

    send_bridge_request("upsert_accounts", serde_json::json!({ "accounts": accounts }))
        .await?;

    let count = entries.len();
    with_state(|state| {
        state.status = format!("已发送 {} 条账号到手表，等待确认...", count);
    });

    Ok(format!("已发送 {} 条账号", count))
}

/// 推送修改的条目到手表（只同步修改项）
pub async fn push_modified_entries(modified: &[TotpEntry]) -> Result<String, String> {
    if modified.is_empty() {
        return Err("没有修改项需要同步".to_string());
    }

    let accounts: Vec<serde_json::Value> = modified
        .iter()
        .map(|e| entry_to_account_value(e))
        .collect();

    send_bridge_request("upsert_accounts", serde_json::json!({ "accounts": accounts }))
        .await?;

    let count = modified.len();
    with_state(|state| {
        state.status = format!("已推送 {} 条修改到手表", count);
        // 更新基准为当前本地
        state.baseline_entries = state.local_entries.clone();
    });

    Ok(format!("已推送 {} 条修改", count))
}

/// 发送增量同步（upsert + remove）到手表
pub async fn send_sync_delta() -> Result<String, String> {
    let (upsert, remove) = get_sync_delta();

    if upsert.is_empty() && remove.is_empty() {
        return Err("没有增量数据需要同步".to_string());
    }

    let upsert_accounts: Vec<serde_json::Value> = upsert
        .iter()
        .map(|e| entry_to_account_value(e))
        .collect();

    let payload = serde_json::json!({
        "type": "sync_delta",
        "upsert": upsert_accounts,
        "remove": remove,
    });

    let payload_str = payload.to_string();
    tracing::info!("send_sync_delta: payload_len={}", payload_str.len());

    log_message("sent", "sync_delta", &format!("upsert={} remove={}", upsert.len(), remove.len()), "pending");

    // 直接使用底层发送，因为 send_bridge_request 会包装 type 字段
    let device_addr = match crate::device::check_device().await {
        Some(addr) => addr,
        None => first_connected_device().await?.addr,
    };
    let pkg_name = match crate::device::resolve_pkg_name(&device_addr).await {
        Some(name) => name,
        None => resolve_target_pkg_name(&device_addr).await
            .unwrap_or_else(|_| DEFAULT_AUTH_PKG_NAME.to_string()),
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
            log_message("sent", "sync_delta", &format!("发送失败: {}", err_detail)[..80], "error");
            Err(format!("发送增量同步失败: {}", err_detail))
        }
    }
}

/// 删除手表端账号
pub async fn remove_account_from_watch(id: &str) -> Result<String, String> {
    send_bridge_request("remove_account", serde_json::json!({ "id": id })).await?;
    Ok(format!("已请求删除账号 {}", id))
}

/// 打开手表端认证器应用
pub async fn launch_auth_app() -> Result<String, String> {
    let dev = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&dev.addr).await?;

    ensure_interconnect_registered(&dev.addr, &pkg_name).await?;
    with_state(|state| {
        state.subscribed = true;
        state.last_device_addr = Some(dev.addr.clone());
        state.resolved_pkg_name = Some(pkg_name.clone());
    });

    let apps = thirdpartyapp::get_thirdparty_app_list(&dev.addr)
        .await
        .map_err(|()| "获取手表应用列表失败".to_string())?;

    let app = apps
        .into_iter()
        .find(|a| a.package_name == pkg_name)
        .ok_or_else(|| format!("手表上未找到认证器应用 ({})", pkg_name))?;

    thirdpartyapp::launch_qa(&dev.addr, &app, "pages/home")
        .await
        .map_err(|()| "启动应用失败".to_string())?;

    Ok(format!("已打开认证器应用"))
}

/// 发送 ping 消息（用于检测连接）
pub async fn ping_watch() -> Result<(), String> {
    let dev = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&dev.addr).await?;
    let payload = serde_json::json!({ "type": "ping" });
    interconnect::send_qaic_message(&dev.addr, &pkg_name, &payload.to_string())
        .await
        .map_err(|e| format!("发送 ping 失败: {:?}", e))?;
    Ok(())
}

/// 将 TotpEntry 转换为手表端期望的 account JSON 格式
fn entry_to_account_value(entry: &TotpEntry) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "id": entry.id,
        "name": entry.name,
        "secret": entry.secret,
        "type": entry.otp_type,
        "algorithm": entry.algorithm,
        "digits": entry.digits,
        "period": entry.period,
    });

    if let Some(ref issuer) = entry.issuer {
        obj["issuer"] = serde_json::json!(issuer);
    }
    if let Some(counter) = entry.counter {
        obj["counter"] = serde_json::json!(counter);
    }
    if let Some(ref extra) = entry.extra_params {
        obj["extraParams"] = extra.clone();
    }
    if let Some(created_at) = entry.created_at {
        obj["createdAt"] = serde_json::json!(created_at);
    }

    obj
}

/// 从事件 payload 中提取实际文本（兼容 payloadText / payload 等包裹层）
fn extract_payload_text(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        // 尝试 payloadText 字段（字符串类型，需二次解析）
        if let Some(text) = json.get("payloadText").and_then(|v| v.as_str()) {
            return text.to_string();
        }
        // 尝试 payload 字段（可能是字符串或对象）
        if let Some(payload_value) = json.get("payload") {
            if let Some(text) = payload_value.as_str() {
                return text.to_string();
            }
            return payload_value.to_string();
        }
        // 尝试 data 字段
        if let Some(data) = json.get("data") {
            if let Some(text) = data.as_str() {
                return text.to_string();
            }
            return data.to_string();
        }
    }
    payload.to_string()
}

/// 处理 interconnect 响应消息
/// 优先支持参考项目格式：{ "data": [...] }
/// 兼容旧格式：{ "type": "list_accounts_result", "accounts": [...] }
pub fn handle_interconnect_response(payload: &str) -> Option<InterconnectResult> {
    tracing::info!("handle_ic: raw_len={}", payload.len());

    let actual_payload = extract_payload_text(payload);
    tracing::info!("handle_ic: extracted={}", &actual_payload[..actual_payload.len().min(300)]);

    let parsed = match serde_json::from_str::<serde_json::Value>(&actual_payload) {
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

    // 参考 Daymatter-AstroBox-Plugin：优先检查 data 字段（账号列表）
    if let Some(data_array) = parsed.get("data").and_then(|v| v.as_array()) {
        let entries: Vec<TotpEntry> = data_array
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
            state.status = format!("已加载 {} 条账号", count);
        });
        return Some(InterconnectResult::AccountList(read_state(|s| s.local_entries.clone())));
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
        "list_accounts_result" => {
            log_message("received", "list_accounts_result", "", "ok");
            mark_sent_ok("list_accounts");
            if let Some(accounts) = parsed.get("accounts").and_then(|v| v.as_array()) {
                let entries: Vec<TotpEntry> = accounts
                    .iter()
                    .filter_map(|a| serde_json::from_value(a.clone()).ok())
                    .collect();
                let count = entries.len();
                tracing::info!("handle_ic: list_accounts_result format, {} entries", count);
                with_state(|state| {
                    state.baseline_entries = entries.clone();
                    state.local_entries = entries;
                    state.loaded = true;
                    state.is_loading = false;
                    state.status = format!("已加载 {} 条账号", count);
                });
                Some(InterconnectResult::AccountList(read_state(|s| s.local_entries.clone())))
            } else if let Some(error) = parsed.get("error").and_then(|v| v.as_str()) {
                with_state(|state| { state.is_loading = false; });
                Some(InterconnectResult::OperationResult {
                    message: format!("手表返回错误: {}", error),
                    is_error: true,
                })
            } else {
                with_state(|state| { state.is_loading = false; });
                None
            }
        }
        "upsert_accounts_result" => {
            log_message("received", "upsert_accounts_result", "", "ok");
            mark_sent_ok("upsert_accounts");
            let message = if parsed.get("error").is_some() {
                format!("同步失败: {}", parsed.get("error").and_then(|v| v.as_str()).unwrap_or("未知错误"))
            } else {
                "同步成功".to_string()
            };
            Some(InterconnectResult::OperationResult { message, is_error: parsed.get("error").is_some() })
        }
        "remove_account_result" => {
            log_message("received", "remove_account_result", "", "ok");
            mark_sent_ok("remove_account");
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

/// 计算本地条目与基准之间的差异
pub fn get_modified_entries() -> Vec<TotpEntry> {
    read_state(|state| {
        let mut modified = Vec::new();

        // 查找新增和修改的条目
        for entry in &state.local_entries {
            if let Some(base) = state.baseline_entries.iter().find(|b| b.id == entry.id) {
                if entry != base {
                    modified.push(entry.clone());
                }
            } else {
                // 新增的条目
                modified.push(entry.clone());
            }
        }

        modified
    })
}

/// 删除本地条目
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

/// 批量删除本地条目（按索引列表，需从大到小排序后删除）
pub fn delete_local_entries(mut indices: Vec<usize>) {
    with_state(|state| {
        indices.sort_by(|a, b| b.cmp(a));
        indices.dedup();
        for index in indices {
            if index < state.local_entries.len() {
                let entry = state.local_entries.remove(index);
                let id = entry.id.clone();
                if !state.deleted_ids.contains(&id) {
                    state.deleted_ids.push(id);
                }
            }
        }
        state.status = "已批量删除本地条目（需保存修改至手表生效）".to_string();
    });
}

/// 按 ID 查找并替换本地条目
pub fn update_local_entry_by_id(id: &str, entry: TotpEntry) {
    with_state(|state| {
        if let Some(idx) = state.local_entries.iter().position(|e| e.id == id) {
            state.local_entries[idx] = entry;
            state.status = "已修改本地条目（需保存修改至手表生效）".to_string();
        }
    });
}

/// 按索引获取本地条目副本
pub fn get_entry_by_index(index: usize) -> Option<TotpEntry> {
    read_state(|state| state.local_entries.get(index).cloned())
}

/// 计算本地条目与基准之间的差异（upsert + remove）
pub fn get_sync_delta() -> (Vec<TotpEntry>, Vec<String>) {
    read_state(|state| {
        let mut upsert = Vec::new();

        // 查找新增和修改的条目
        for entry in &state.local_entries {
            if let Some(base) = state.baseline_entries.iter().find(|b| b.id == entry.id) {
                if entry != base {
                    upsert.push(entry.clone());
                }
            } else {
                // 新增的条目
                upsert.push(entry.clone());
            }
        }

        (upsert, state.deleted_ids.clone())
    })
}

/// 同步成功后清空增量记录
pub fn clear_sync_delta() {
    with_state(|state| {
        state.deleted_ids.clear();
        state.baseline_entries = state.local_entries.clone();
    });
}

/// 更新本地条目
pub fn update_local_entry(index: usize, entry: TotpEntry) {
    with_state(|state| {
        if index < state.local_entries.len() {
            state.local_entries[index] = entry;
            state.status = "已修改本地条目（需保存修改至手表生效）".to_string();
        }
    });
}

/// 添加条目到本地列表
pub fn add_local_entry(entry: TotpEntry) {
    with_state(|state| {
        state.local_entries.push(entry);
        state.status = "已添加到本地列表（需保存修改至手表生效）".to_string();
    });
}

/// 解析 otpauth:// URI 为 TotpEntry
pub fn parse_otpauth_uri(uri: &str) -> Option<TotpEntry> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = url::Url::parse(trimmed).ok()?;
    if parsed.scheme() != "otpauth" {
        return None;
    }

    let otp_type = parsed.host_str()?.to_lowercase();
    if otp_type != "totp" && otp_type != "hotp" {
        return None;
    }

    let label = percent_decode(parsed.path().trim_start_matches('/'));
    if label.trim().is_empty() {
        return None;
    }

    let (issuer_from_path, account) = match label.split_once(':') {
        Some((issuer, acc)) if !acc.trim().is_empty() => {
            (issuer.trim().to_string(), acc.trim().to_string())
        }
        _ => (String::new(), label.trim().to_string()),
    };

    let secret = get_query_param(&parsed, "secret")?;
    let issuer_from_query = get_query_param(&parsed, "issuer");
    let issuer = issuer_from_query
        .filter(|v| !v.trim().is_empty())
        .or_else(|| if issuer_from_path.is_empty() { None } else { Some(issuer_from_path) });

    let algorithm = get_query_param(&parsed, "algorithm").unwrap_or_else(|| "SHA1".to_string());
    let digits: u32 = get_query_param(&parsed, "digits")
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    let period: u32 = get_query_param(&parsed, "period")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let counter = get_query_param(&parsed, "counter").and_then(|v| v.parse().ok());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Some(TotpEntry {
        id: format!("acc_{}_{}", now, rand_simple()),
        name: account,
        issuer,
        secret,
        otp_type,
        algorithm,
        digits,
        period,
        counter,
        extra_params: None,
        created_at: Some(now),
    })
}

/// 简单随机数生成（避免引入 rand 库）
fn rand_simple() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// 批量解析文本中的 otpauth:// URI
pub fn parse_otpauth_text(text: &str) -> Vec<TotpEntry> {
    extract_otpauth_candidates(text)
        .into_iter()
        .filter_map(|candidate| parse_otpauth_uri(&candidate))
        .collect()
}

fn extract_otpauth_candidates(text: &str) -> Vec<String> {
    let normalized = text.replace(r"\/", "/").replace("&amp;", "&");
    let mut candidates = Vec::new();

    for line in normalized.lines() {
        let line = line.trim();
        if line.starts_with("otpauth://totp/") || line.starts_with("otpauth://hotp/") {
            candidates.push(line.to_string());
        }
    }

    let mut rest = normalized.as_str();
    while let Some(start) = rest.find("otpauth://") {
        let candidate_start = &rest[start..];
        let end = candidate_start
            .char_indices()
            .find_map(|(idx, ch)| is_uri_delimiter(ch).then_some(idx))
            .unwrap_or(candidate_start.len());
        candidates.push(candidate_start[..end].to_string());
        rest = &candidate_start[end..];
    }

    let mut unique = Vec::new();
    for candidate in candidates {
        let candidate = normalize_uri_candidate(&candidate);
        if !candidate.is_empty() && !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

fn is_uri_delimiter(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '<' | '>' | ',' | ')' | ']' | '}')
}

fn normalize_uri_candidate(candidate: &str) -> String {
    candidate
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '<' | '>' | ',' | ')' | ']' | '}'))
        .to_string()
}

fn get_query_param(uri: &url::Url, key: &str) -> Option<String> {
    uri.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[idx + 1]), hex_value(bytes[idx + 2])) {
                output.push((hi << 4) | lo);
                idx += 3;
                continue;
            }
        }
        output.push(bytes[idx]);
        idx += 1;
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// 将 TotpEntry 转换为 otpauth:// URI
pub fn entry_to_otpauth_uri(entry: &TotpEntry) -> String {
    let label = if let Some(ref issuer) = entry.issuer {
        if !issuer.is_empty() {
            format!("{}:{}", issuer, entry.name)
        } else {
            entry.name.clone()
        }
    } else {
        entry.name.clone()
    };

    // 使用 url crate 的 percent_encoding 替代 urlencoding
    let encoded_label = percent_encode_component(&label);
    let mut params = vec![
        format!("secret={}", entry.secret),
        format!("algorithm={}", entry.algorithm),
        format!("digits={}", entry.digits),
        format!("period={}", entry.period),
    ];

    if let Some(ref issuer) = entry.issuer {
        if !issuer.is_empty() {
            params.push(format!("issuer={}", issuer));
        }
    }

    if entry.otp_type == "hotp" {
        if let Some(counter) = entry.counter {
            params.push(format!("counter={}", counter));
        }
    }

    format!("otpauth://{}/{}?{}", entry.otp_type, encoded_label, params.join("&"))
}

/// 简易 percent-encode，只编码非保留字符
fn percent_encode_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '@' | '+') {
                ch.to_string()
            } else {
                let bytes = ch.to_string().into_bytes();
                bytes
                    .iter()
                    .map(|b| format!("%{:02X}", b))
                    .collect()
            }
        })
        .collect()
}

/// 批量导出所有本地条目为 otpauth:// URI 列表
pub fn export_all_as_uris() -> Vec<String> {
    read_state(|state| {
        state
            .local_entries
            .iter()
            .map(|e| entry_to_otpauth_uri(e))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_totp_uri() {
        let entry = parse_otpauth_uri(
            "otpauth://totp/GitHub:alice?secret=ABC123&issuer=GitHub&algorithm=SHA1&digits=6&period=30",
        )
        .expect("should parse");

        assert_eq!(entry.name, "alice");
        assert_eq!(entry.issuer.as_deref(), Some("GitHub"));
        assert_eq!(entry.secret, "ABC123");
        assert_eq!(entry.algorithm, "SHA1");
        assert_eq!(entry.digits, 6);
        assert_eq!(entry.period, 30);
    }

    #[test]
    fn parses_hotp_uri() {
        let entry = parse_otpauth_uri(
            "otpauth://hotp/Test:user?secret=XYZ789&counter=5&digits=8",
        )
        .expect("should parse");

        assert_eq!(entry.otp_type, "hotp");
        assert_eq!(entry.counter, Some(5));
        assert_eq!(entry.digits, 8);
    }

    #[test]
    fn extracts_multiple_uris() {
        let text = r#"
        otpauth://totp/GitHub:alice?secret=AAA111&issuer=GitHub
        otpauth://totp/Bob?secret=BBB222&issuer=Mail
        "#;

        let entries = parse_otpauth_text(text);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn roundtrip_otpauth_uri() {
        let entry = parse_otpauth_uri(
            "otpauth://totp/TestService:test@example.com?secret=JBSWY3DPEHPK3PXP&issuer=TestService&algorithm=SHA256&digits=8&period=60",
        )
        .expect("should parse");

        let uri = entry_to_otpauth_uri(&entry);
        let reparsed = parse_otpauth_uri(&uri).expect("should round-trip");

        assert_eq!(entry.name, reparsed.name);
        assert_eq!(entry.secret, reparsed.secret);
        assert_eq!(entry.algorithm, reparsed.algorithm);
        assert_eq!(entry.digits, reparsed.digits);
        assert_eq!(entry.period, reparsed.period);
    }

    #[test]
    fn parses_list_accounts_result() {
        let payload = serde_json::json!({
            "payloadText": serde_json::json!({
                "type": "list_accounts_result",
                "accounts": [{
                    "id": "acc_1",
                    "name": "alice",
                    "issuer": "GitHub",
                    "secret": "ABC123",
                    "type": "totp",
                    "algorithm": "SHA1",
                    "digits": 6,
                    "period": 30,
                    "createdAt": 1
                }]
            }).to_string()
        })
        .to_string();

        let result = handle_interconnect_response(&payload);
        match result {
            Some(InterconnectResult::AccountList(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].name, "alice");
                assert_eq!(entries[0].issuer.as_deref(), Some("GitHub"));
            }
            _ => panic!("expected account list"),
        }
    }

    #[test]
    fn parses_bridge_ready_status() {
        let payload = serde_json::json!({
            "payload": serde_json::json!({
                "type": "bridge_status",
                "phase": "ready",
                "detail": "bridge initialized"
            })
        })
        .to_string();

        let result = handle_interconnect_response(&payload);
        match result {
            Some(InterconnectResult::OperationResult { message, is_error }) => {
                assert!(!is_error);
                assert!(message.contains("ready"));
            }
            _ => panic!("expected bridge ready operation result"),
        }
    }
}
