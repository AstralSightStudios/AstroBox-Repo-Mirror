//! 事件处理：SF 协议分发、握手状态机、设备/应用管理、UI 事件。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::astrobox::psys_host;
use crate::astrobox::psys_host::dialog;
use crate::astrobox::psys_host::interconnect;
use crate::astrobox::psys_host::register;
use crate::astrobox::psys_host::thirdpartyapp;
use crate::astrobox::psys_host::timer;
use crate::ui::api_client;
use crate::ui::state::{self, AppConnectionStatus, InstalledApp, MainTab};

// ========== 事件ID常量 ==========

pub const TAB_CONNECT_EVENT: &str = "tab_connect";
pub const TAB_ABOUT_EVENT: &str = "tab_about";
pub const EVENT_REFRESH_DEVICES: &str = "action:devices.refresh";
pub const APP_CONNECT_PREFIX: &str = "app:connect:";
pub const APP_DISCONNECT_PREFIX: &str = "app:disconnect:";
pub const OPEN_HELP_DOC_EVENT: &str = "open_help_doc";
pub const OPEN_QQ_GROUP_EVENT: &str = "open_qq_group";

/// 握手超时（毫秒）
const HANDSHAKE_TIMEOUT_MS: u64 = 5000;
/// 启动快应用后等待其就绪的时间
const LAUNCH_READY_WAIT_MS: u64 = 2000;

// ========== SSE 取消管理 ==========

static SSE_CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
/// 按 (设备地址, 包名) 索引的活跃 SSE 请求ID集合，断开时统一取消
static SSE_BY_APP: OnceLock<Mutex<HashMap<(String, String), HashSet<String>>>> = OnceLock::new();

fn sse_cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    SSE_CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sse_by_app() -> &'static Mutex<HashMap<(String, String), HashSet<String>>> {
    SSE_BY_APP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_sse(addr: &str, pkg: &str, id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    sse_cancels()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id.to_string(), flag.clone());
    sse_by_app()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .entry((addr.to_string(), pkg.to_string()))
        .or_default()
        .insert(id.to_string());
    flag
}

fn unregister_sse(addr: &str, pkg: &str, id: &str) {
    sse_cancels()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(id);
    if let Some(set) = sse_by_app()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(&(addr.to_string(), pkg.to_string()))
    {
        set.remove(id);
    }
}

/// 取消某应用下所有活跃 SSE 流
fn cancel_all_sse_for_app(addr: &str, pkg: &str) {
    let ids: Vec<String> = sse_by_app()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&(addr.to_string(), pkg.to_string()))
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    let cancels = sse_cancels().lock().unwrap_or_else(|p| p.into_inner());
    for id in &ids {
        if let Some(flag) = cancels.get(id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

// ========== Interconnect 消息处理 ==========

struct ParsedMessage {
    addr: String,
    pkg_name: String,
    data: String,
}

fn parse_message(payload: &str) -> ParsedMessage {
    let envelope: Option<Value> = serde_json::from_str(payload).ok();
    let data = envelope
        .as_ref()
        .and_then(|v| v.get("payloadText").and_then(|x| x.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| payload.to_string());

    let pkg_name = envelope
        .as_ref()
        .and_then(|v| v.get("pkgName").and_then(|x| x.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default();

    let addr = envelope
        .as_ref()
        .and_then(|v| v.get("addr").and_then(|x| x.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default();

    ParsedMessage {
        addr,
        pkg_name,
        data,
    }
}

pub fn handle_interconnect_message(payload: &str) {
    let parsed = parse_message(payload);
    if parsed.addr.is_empty() || parsed.pkg_name.is_empty() {
        tracing::warn!("丢弃无法定位来源(addr/pkg)的互联消息");
        return;
    }

    let msg: Value = match serde_json::from_str(&parsed.data) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("解析SF消息失败: {} raw={}", e, parsed.data);
            return;
        }
    };

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let status_str = msg.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let data = msg.get("data").cloned();

    // 快应用可主动发起握手：即使当前未连接也放行 SF_HANDSHAKE
    if msg_type == "SF_HANDSHAKE" {
        handle_incoming_handshake(&parsed.addr, &parsed.pkg_name);
        crate::ui::build::render_without_auto_refresh();
        return;
    }

    // 其他消息只处理已连接/握手中的应用
    let status = state::connection_status(&parsed.addr, &parsed.pkg_name);
    if status == AppConnectionStatus::Disconnected || status == AppConnectionStatus::Failed {
        if !state::is_handshake_pending(&parsed.addr, &parsed.pkg_name) {
            tracing::warn!(
                "丢弃未连接应用的消息: type={} pkg={} addr={}",
                msg_type, parsed.pkg_name, parsed.addr
            );
            return;
        }
    }

    tracing::info!(
        "SF消息: type={} status={} pkg={} addr={}",
        msg_type,
        status_str,
        parsed.pkg_name,
        parsed.addr
    );

    dispatch_sf(&parsed.addr, &parsed.pkg_name, msg_type, status_str, data);
    crate::ui::build::render_without_auto_refresh();
}

fn dispatch_sf(addr: &str, pkg: &str, msg_type: &str, status: &str, data: Option<Value>) {
    match msg_type {
        "SF_HANDSHAKE_ACK" => {
            if status == "OK" {
                handle_handshake_ack(addr, pkg);
            }
        }
        "SF_PING" => {
            // 记录心跳时间，用于心跳超时检测
            state::record_ping(addr, pkg);
            // 心跳：原样回传 ts
            let ts = data
                .as_ref()
                .and_then(|d| d.get("ts"))
                .cloned()
                .unwrap_or(json!(0));
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_PONG",
                    "status": "OK",
                    "data": { "ts": ts }
                }),
            );
        }
        "SF_REQUEST" => {
            if let Some(d) = data {
                handle_sf_request(addr, pkg, d);
            }
        }
        "SF_CLOSE_BRIDGE_ACK" => {
            // 快应用确认关闭桥接
            tracing::info!("收到 SF_CLOSE_BRIDGE_ACK: pkg={} addr={}", pkg, addr);
            state::set_connection_status(addr, pkg, AppConnectionStatus::Disconnected);
            state::persist_now();
            crate::ui::build::render_without_auto_refresh();
        }
        "SF_CLOSE" => {
            if let Some(id) = data
                .and_then(|d| d.get("id").and_then(|v| v.as_str()).map(String::from))
            {
                cancel_sse(&id);
                tracing::info!("收到SF_CLOSE，已取消请求: id={}", id);
            }
        }
        _ => {
            tracing::info!("未处理的SF消息类型: {}", msg_type);
        }
    }
}

// ========== 握手 ==========

/// 握手成功（收到 ACK）
fn handle_handshake_ack(addr: &str, pkg: &str) {
    // 取出待确认握手并清除超时定时器
    if let Some(timer_id) = state::take_pending_handshake(addr, pkg) {
        clear_timer(timer_id);
    } else {
        // 没有待确认握手，可能是重复 ACK，忽略
        tracing::debug!("收到非预期的 SF_HANDSHAKE_ACK: pkg={}", pkg);
        return;
    }

    state::set_connection_status(addr, pkg, AppConnectionStatus::Connected);
    state::persist_now();
    crate::ui::build::render_without_auto_refresh();

    tracing::info!("握手成功: addr={} pkg={}", addr, pkg);
    // 弹窗显示 应用名(包名)
    let app_label = state::with_state(|s| {
        s.installed_apps
            .iter()
            .find(|a| a.addr == addr && a.package_name == pkg)
            .map(|a| {
                if a.app_name.is_empty() {
                    a.package_name.clone()
                } else {
                    format!("{}({})", a.app_name, a.package_name)
                }
            })
            .unwrap_or_else(|| pkg.to_string())
    });
    show_alert("连接成功", &app_label);
}

/// 快应用主动发起握手：注册接收器并回复 ACK，建立连接
fn handle_incoming_handshake(addr: &str, pkg: &str) {
    tracing::info!("收到快应用主动握手: addr={} pkg={}", addr, pkg);
    // 若有插件发起的待确认握手，清除其超时定时器（已被对端握手满足）
    if let Some(timer_id) = state::take_pending_handshake(addr, pkg) {
        clear_timer(timer_id);
    }
    // 注册接收器，确保能收到后续消息
    let addr_owned = addr.to_string();
    let pkg_owned = pkg.to_string();
    wit_bindgen::block_on(async move {
        let _ = register::register_interconnect_recv(&addr_owned, &pkg_owned).await;
    });

    state::set_connection_status(addr, pkg, AppConnectionStatus::Connected);
    state::record_ping(addr, pkg);
    state::persist_now();

    // 回复握手确认
    send_json(
        addr,
        pkg,
        json!({
            "type": "SF_HANDSHAKE_ACK",
            "status": "OK",
            "data": {}
        }),
    );
    crate::ui::build::render_without_auto_refresh();
}

/// 握手超时定时器触发
fn handle_handshake_timeout(addr: &str, pkg: &str) {
    // 仅当仍在握手中才判定失败
    if !state::is_handshake_pending(addr, pkg) {
        return;
    }
    state::take_pending_handshake(addr, pkg);
    let reason = "握手超时，未收到响应";
    state::set_connection_failed(addr, pkg, reason);
    crate::ui::build::render_without_auto_refresh();
    tracing::warn!("握手超时: addr={} pkg={}", addr, pkg);
    show_alert("连接失败", reason);
}

/// 发起连接：注册接收器、启动快应用、发送握手、启动超时定时器
fn connect_app(app: &InstalledApp) {
    let addr = app.addr.clone();
    let pkg = app.package_name.clone();
    let app_name = app.app_name.clone();
    let version_code = app.version_code;

    // 立即切换为握手中状态
    state::set_connection_status(&addr, &pkg, AppConnectionStatus::Handshaking);
    crate::ui::build::render_without_auto_refresh();

    wit_bindgen::block_on(async move {
        // 1. 注册接收器
        let _ = register::register_interconnect_recv(&addr, &pkg).await;

        // 2. 启动快应用
        let app_info = thirdpartyapp::AppInfo {
            package_name: pkg.clone(),
            fingerprint: Vec::new(),
            version_code,
            can_remove: true,
            app_name,
        };
        match thirdpartyapp::launch_qa(&addr, &app_info, "/index").await {
            Ok(_) => tracing::info!("已启动快应用: {}", pkg),
            Err(()) => {
                tracing::error!("启动快应用失败: {}", pkg);
                state::set_connection_failed(&addr, &pkg, "启动快应用失败");
                crate::ui::build::render_without_auto_refresh();
                show_alert("连接失败", "启动快应用失败，请确认应用已安装");
                return;
            }
        }

        // 3. 等待快应用就绪
        std::thread::sleep(Duration::from_millis(LAUNCH_READY_WAIT_MS));

        // 4. 发送握手
        send_json(
            &addr,
            &pkg,
            json!({
                "type": "SF_HANDSHAKE",
                "data": {}
            }),
        );

        // 5. 启动超时定时器
        let payload = json!({
            "kind": HS_TIMEOUT_KIND,
            "addr": addr,
            "pkg": pkg
        })
        .to_string();
        let timer_id = timer::set_timeout(HANDSHAKE_TIMEOUT_MS, &payload).await;
        // 若已有旧定时器，先清除
        if let Some(old) = state::insert_pending_handshake(&addr, &pkg, timer_id) {
            clear_timer(old);
        }
    });
}

/// 断开连接：取消握手/SSE，状态置为 Disconnected
fn disconnect_app(app: &InstalledApp) {
    let addr = &app.addr;
    let pkg = &app.package_name;

    // 取消待确认握手
    if let Some(timer_id) = state::take_pending_handshake(addr, pkg) {
        clear_timer(timer_id);
    }

    // 取消该应用下所有 SSE 流
    cancel_all_sse_for_app(addr, pkg);

    // 通知快应用关闭桥接（快应用会回 SF_CLOSE_BRIDGE_ACK，届时最终置为 Disconnected）。
    // 仅当当前处于已连接状态时发送；若仍在握手中则不发送。
    if state::connection_status(addr, pkg) == AppConnectionStatus::Connected {
        tracing::info!("发送 SF_CLOSE_BRIDGE: addr={} pkg={}", addr, pkg);
        send_json(
            addr,
            pkg,
            json!({ "type": "SF_CLOSE_BRIDGE", "status": "OK", "data": {} }),
        );
        // 先置为 Disconnected（ACK 到达时会再次确认），UI 立即响应断开
        state::set_connection_status(addr, pkg, AppConnectionStatus::Disconnected);
    } else {
        state::set_connection_status(addr, pkg, AppConnectionStatus::Disconnected);
    }
    state::persist_now();
    crate::ui::build::render_without_auto_refresh();
    tracing::info!("已断开: addr={} pkg={}", addr, pkg);
}

// ========== SF_REQUEST 处理 ==========

fn handle_sf_request(addr: &str, pkg: &str, data: Value) {
    let id = data.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let url = match data.get("url").and_then(|v| v.as_str()) {
        Some(u) => u.to_string(),
        None => {
            send_error(addr, pkg, &id, 0, "请求缺少url字段");
            return;
        }
    };
    let method = data
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_string();
    let is_sse = data.get("sse").and_then(|v| v.as_bool()).unwrap_or(false);
    let timeout_ms = data
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(15000) as u32;

    let headers = parse_headers(data.get("headers"));
    let body_bytes = data
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| s.as_bytes().to_vec());

    state::record_request(addr, pkg, Some(&url));

    if is_sse {
        run_sse(
            addr.to_string(),
            pkg.to_string(),
            id,
            method,
            url,
            headers,
            body_bytes,
            timeout_ms,
        );
    } else {
        // 普通请求同步执行（与参考实现一致）：WASI HTTP 的阻塞等待会驱动事件循环，
        // 不会导致心跳永久卡死。spawn 脱离 on_event 的根任务后反而可能不被轮询。
        execute_and_respond(
            addr, pkg, &id, &method, &url, &headers, body_bytes.as_deref(), timeout_ms,
        );
    }
}

fn execute_and_respond(
    addr: &str,
    pkg: &str,
    id: &str,
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
    timeout_ms: u32,
) {
    match api_client::execute_request(method, url, headers, body, timeout_ms) {
        Ok(resp) => {
            let ok = (200..300).contains(&resp.status_code);
            tracing::info!(
                "请求完成: id={} status={} body_len={}",
                id, resp.status_code, resp.body.len()
            );
            state::record_result(addr, pkg, ok, Some(format!("HTTP {}", resp.status_code)));
            let resp_headers = filter_response_headers(&resp.headers);
            send_response(addr, pkg, id, resp.status_code, &resp_headers, &resp.body);
        }
        Err(e) => {
            tracing::error!("HTTP请求失败: id={} err={}", id, e);
            state::record_result(addr, pkg, false, Some(e.clone()));
            let err_msg = classify_network_error(&e);
            send_error(addr, pkg, id, 0, &err_msg);
        }
    }
}

fn send_response(
    addr: &str,
    pkg: &str,
    id: &str,
    status_code: u16,
    headers: &HashMap<String, String>,
    body: &[u8],
) {
    // 小数据且为 UTF-8：单条文本返回
    if body.len() <= 16 * 1024 {
        if let Ok(text) = std::str::from_utf8(body) {
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_RESPONSE",
                    "status": "OK",
                    "data": {
                        "id": id,
                        "statusCode": status_code,
                        "headers": headers,
                        "body": text,
                        "chunk": 0,
                        "totalChunks": 0
                    }
                }),
            );
            return;
        }
    }

    // 大数据：base64 分片
    let encoded = base64::engine::general_purpose::STANDARD.encode(body);
    let chunk_size = 12 * 1024;
    let total_chunks = (encoded.len() + chunk_size - 1) / chunk_size;

    for i in 0..total_chunks {
        let start = i * chunk_size;
        let end = (start + chunk_size).min(encoded.len());
        let chunk_body = &encoded[start..end];
        let chunk_num = i + 1;

        if chunk_num == 1 {
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_RESPONSE",
                    "status": "OK",
                    "data": {
                        "id": id,
                        "statusCode": status_code,
                        "headers": headers,
                        "body": chunk_body,
                        "chunk": chunk_num,
                        "totalChunks": total_chunks
                    }
                }),
            );
        } else {
            send_json(
                addr,
                pkg,
                json!({
                    "type": "SF_RESPONSE",
                    "status": "OK",
                    "data": {
                        "id": id,
                        "body": chunk_body,
                        "chunk": chunk_num,
                        "totalChunks": total_chunks
                    }
                }),
            );
        }
    }
}

fn send_error(addr: &str, pkg: &str, id: &str, status_code: u16, error: &str) {
    send_json(
        addr,
        pkg,
        json!({
            "type": "SF_RESPONSE",
            "status": error,
            "data": {
                "id": id,
                "statusCode": status_code,
                "error": error
            }
        }),
    );
}

// ========== SSE ==========

fn run_sse(
    addr: String,
    pkg: String,
    id: String,
    method: String,
    url: String,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
    timeout_ms: u32,
) {
    let cancel = register_sse(&addr, &pkg, &id);

    let cancel_read = cancel.clone();
    let id_inner = id.clone();
    let addr_inner = addr.clone();
    let pkg_inner = pkg.clone();

    wit_bindgen::spawn(async move {
        let result = api_client::execute_sse(
            &method,
            &url,
            &headers,
            body.as_deref(),
            timeout_ms,
            &mut |event, data| {
                send_json(
                    &addr_inner,
                    &pkg_inner,
                    json!({
                        "type": "SF_SSE_EVENT",
                        "status": "OK",
                        "data": {
                            "id": id_inner,
                            "event": if event.is_empty() { "message" } else { event },
                            "data": data
                        }
                    }),
                );
                !cancel_read.load(Ordering::SeqCst)
            },
            &|| cancel_read.load(Ordering::SeqCst),
        );

        unregister_sse(&addr, &pkg, &id);

        match result {
            Ok(()) => {
                send_json(
                    &addr,
                    &pkg,
                    json!({ "type": "SF_SSE_END", "status": "OK", "data": { "id": id } }),
                );
                state::record_result(&addr, &pkg, true, Some("SSE完成".into()));
            }
            Err(e) => {
                if cancel.load(Ordering::SeqCst) {
                    send_json(
                        &addr,
                        &pkg,
                        json!({ "type": "SF_SSE_END", "status": "OK", "data": { "id": id } }),
                    );
                } else {
                    tracing::error!("SSE错误: {}", e);
                    state::record_result(&addr, &pkg, false, Some(e.clone()));
                    send_json(
                        &addr,
                        &pkg,
                        json!({
                            "type": "SF_SSE_ERROR",
                            "status": e,
                            "data": { "id": id }
                        }),
                    );
                }
            }
        }
        crate::ui::build::render_without_auto_refresh();
    });
}

fn cancel_sse(id: &str) {
    if let Some(flag) = sse_cancels()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id)
    {
        flag.store(true, Ordering::SeqCst);
    }
}

// ========== 设备/应用管理 ==========

/// 启动时初始刷新：填充设备/应用缓存，并启动心跳检测定时器
pub fn initial_refresh() {
    refresh_connected_devices();
    refresh_installed_apps();

    // 启动心跳检测定时器
    start_heartbeat_timer();
}

/// 启动心跳检测 interval（每 3 秒检查一次）
fn start_heartbeat_timer() {
    let payload = json!({ "kind": HEARTBEAT_KIND }).to_string();
    wit_bindgen::block_on(async move {
        let id = timer::set_interval(HEARTBEAT_INTERVAL_MS, &payload).await;
        state::set_heartbeat_timer(id);
    });
}

/// 自动刷新（节流）
pub fn auto_refresh() -> bool {
    if !state::try_claim_auto_refresh(1500) {
        return false;
    }
    refresh_connected_devices();
    refresh_installed_apps();
    true
}

/// 手动刷新设备与应用
pub fn refresh_device_list() {
    state::set_devices_loading(true);
    state::set_apps_loading(true);
    crate::ui::build::render_without_auto_refresh();

    refresh_connected_devices();
    refresh_installed_apps();

    state::set_devices_loading(false);
    state::set_apps_loading(false);

    let device_count = state::connected_devices().len();
    let app_count = state::installed_apps().len();
    crate::ui::build::render_without_auto_refresh();
    tracing::info!("刷新完成：{} 台设备，{} 个应用", device_count, app_count);
}

fn refresh_connected_devices() {
    let devices = wit_bindgen::block_on(async move {
        psys_host::device::get_connected_device_list().await
    });
    let list: Vec<(String, String)> = devices
        .into_iter()
        .map(|info| (info.addr, info.name))
        .collect();

    // 处理断开的设备：清除其待确认握手、重置连接状态
    let new_addrs: HashSet<String> = list.iter().map(|(a, _)| a.clone()).collect();
    let old_addrs: HashSet<String> = state::connected_devices()
        .iter()
        .map(|(a, _)| a.clone())
        .collect();
    for gone in old_addrs.difference(&new_addrs) {
        for timer_id in state::drain_pending_for_device(gone) {
            clear_timer(timer_id);
        }
        // SSE 任务会因连接丢失而结束；状态在 prune 时清理
    }

    tracing::info!("已连接设备: {} 台", list.len());
    state::set_connected_devices(list);
}

fn refresh_installed_apps() {
    let devices = state::connected_devices();
    let mut out: Vec<InstalledApp> = Vec::new();

    for (addr, device_name) in &devices {
        let addr_for_call = addr.clone();
        let result = wit_bindgen::block_on(async move {
            thirdpartyapp::get_thirdparty_app_list(&addr_for_call).await
        });
        if let Ok(apps) = result {
            for app in apps {
                out.push(InstalledApp {
                    addr: addr.clone(),
                    device_name: device_name.clone(),
                    package_name: app.package_name,
                    app_name: app.app_name,
                    version_code: app.version_code,
                });
            }
        }
    }

    out.sort_by(|a, b| {
        a.device_name
            .cmp(&b.device_name)
            .then(a.app_name.cmp(&b.app_name))
            .then(a.package_name.cmp(&b.package_name))
    });

    tracing::info!("设备应用列表: {} 个", out.len());
    state::set_installed_apps(out);
    state::prune_stale_connections();
}


fn clear_timer(timer_id: u64) {
    wit_bindgen::block_on(async move {
        timer::clear_timer(timer_id).await;
    });
}

// ========== 定时器事件 ==========

/// 心跳检测间隔
const HEARTBEAT_INTERVAL_MS: u64 = 3000;
/// 心跳超时阈值（超过此时长未收到 PING 则断开）。
/// 文档：快应用每10秒发一次 PING，5秒内未收到回复会断开。这里给足余量。
const HEARTBEAT_TIMEOUT_MS: u128 = 30_000;
/// 定时器载荷中标记握手超时的 kind
const HS_TIMEOUT_KIND: &str = "hs_timeout";
/// 定时器载荷中标记心跳检测的 kind
const HEARTBEAT_KIND: &str = "heartbeat";

pub fn handle_timer_payload(payload: &str) {
    // lib.rs 已从宿主信封 {"timerId":..,"kind":..,"payload":"..."} 中
    // 提取出 payload 字符串传入，这里直接解析业务载荷。
    let Ok(json) = serde_json::from_str::<Value>(payload) else {
        return;
    };
    let kind = json.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        HS_TIMEOUT_KIND => {
            let addr = json.get("addr").and_then(|v| v.as_str()).unwrap_or("");
            let pkg = json.get("pkg").and_then(|v| v.as_str()).unwrap_or("");
            if !addr.is_empty() && !pkg.is_empty() {
                handle_handshake_timeout(addr, pkg);
            }
        }
        HEARTBEAT_KIND => {
            check_heartbeat_timeout();
        }
        _ => {}
    }
}

/// 检查所有已连接应用的心跳，超时的发送 SF_CLOSE_BRIDGE 并断开
fn check_heartbeat_timeout() {
    let stale = state::stale_connections(HEARTBEAT_TIMEOUT_MS);
    let any = !stale.is_empty();
    for (addr, pkg) in stale {
        tracing::warn!("心跳超时，断开连接: addr={} pkg={}", addr, pkg);
        // 通知快应用关闭桥接
        send_json(
            &addr,
            &pkg,
            json!({ "type": "SF_CLOSE_BRIDGE", "status": "OK", "data": {} }),
        );
        state::set_connection_status(&addr, &pkg, AppConnectionStatus::Disconnected);
        state::persist_now();
    }
    if any {
        crate::ui::build::render_without_auto_refresh();
    }
}

// ========== UI事件 ==========

pub fn ui_event_processor(
    _event_type: crate::exports::astrobox::psys_plugin::event_v3::Event,
    event_id: &str,
    _event_payload: &str,
) {
    tracing::info!("UI事件: id={}", event_id);

    if let Some(idx_str) = event_id.strip_prefix(APP_CONNECT_PREFIX) {
        if let Ok(idx) = idx_str.parse::<usize>() {
            let apps = state::installed_apps();
            if let Some(app) = apps.get(idx) {
                connect_app(app);
            }
        }
        return;
    }

    if let Some(idx_str) = event_id.strip_prefix(APP_DISCONNECT_PREFIX) {
        if let Ok(idx) = idx_str.parse::<usize>() {
            let apps = state::installed_apps();
            if let Some(app) = apps.get(idx) {
                disconnect_app(app);
            }
        }
        return;
    }

    match event_id {
        TAB_CONNECT_EVENT => switch_tab(MainTab::Connect),
        TAB_ABOUT_EVENT => switch_tab(MainTab::About),
        EVENT_REFRESH_DEVICES => refresh_device_list(),
        OPEN_HELP_DOC_EVENT => {
            dialog::open_url("https://docs.b4qaq.cn/docs/simplefetch");
        }
        OPEN_QQ_GROUP_EVENT => {
            dialog::open_url("http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=1vc4XKmAyGeJJTmXumfkaaxRcQl1hMaK");
        }
        _ => {}
    }
}

fn switch_tab(tab: MainTab) {
    state::with_state(|s| s.current_tab = tab);
    crate::ui::build::render_without_auto_refresh();
}

// ========== 辅助函数 ==========

fn parse_headers(value: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(obj) = value.and_then(|v| v.as_object()) {
        for (k, v) in obj {
            let s = if let Some(s) = v.as_str() {
                s.to_string()
            } else {
                v.to_string()
            };
            map.insert(k.clone(), s);
        }
    }
    map
}

fn filter_response_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    const HOP_BY_HOP: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
        "content-encoding",
        "content-length",
    ];
    headers
        .iter()
        .filter(|(k, _)| !HOP_BY_HOP.contains(&k.to_lowercase().as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn classify_network_error(e: &str) -> String {
    let lower = e.to_lowercase();
    if lower.contains("timeout") || lower.contains("超时") {
        "请求超时".to_string()
    } else if lower.contains("dns") || lower.contains("resolve") {
        "DNS解析失败".to_string()
    } else if lower.contains("refused") || lower.contains("拒绝") {
        "连接被拒绝".to_string()
    } else if lower.contains("unreachable") || lower.contains("network") {
        "网络不可达".to_string()
    } else {
        format!("网络错误: {}", e)
    }
}

fn send_json(addr: &str, pkg_name: &str, message: Value) {
    let text = message.to_string();
    // 打印发送给快应用的数据
    tracing::info!("发送 → pkg={} addr={} data={}", pkg_name, addr, text);
    let addr_owned = addr.to_string();
    let pkg_owned = pkg_name.to_string();
    let result = wit_bindgen::block_on(async move {
        interconnect::send_qaic_message(&addr_owned, &pkg_owned, &text).await
    });
    if let Err(e) = result {
        tracing::error!(
            "发送SF消息失败: addr={} pkg={} err={:?}",
            addr,
            pkg_name,
            e
        );
    }
}

fn show_alert(title: &str, message: &str) {
    let title_str = title.to_string();
    let message_str = message.to_string();
    wit_bindgen::block_on(async move {
        let _ = dialog::show_dialog(
            dialog::DialogType::Alert,
            dialog::DialogStyle::Website,
            &dialog::DialogInfo {
                title: title_str,
                content: message_str,
                buttons: vec![dialog::DialogButton {
                    id: "ok".to_string(),
                    primary: true,
                    content: "确定".to_string(),
                }],
            },
        )
        .await;
    });
}
