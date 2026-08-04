use astrobox_ng_wit::astrobox::psys_host::{
    device, dialog, interconnect, os, register, thirdpartyapp,
};
use serde_json::Value;

use crate::protocol::{
    self, EXEC_ACK, EXEC_RESULT, HANDSHAKE, HEARTBEAT, HEARTBEAT_ACK, QUICK_APP_PACKAGE,
    RAW_LIST_DATA, REQUEST_RAW_LIST, REQUEST_SCREENSHOT_LIST, SCREENSHOT_CHUNK_FINISH,
    SCREENSHOT_CHUNK_PART, SCREENSHOT_CHUNK_START, SCREENSHOT_FETCH_PROGRESS,
    SCREENSHOT_FETCH_RESULT, SCREENSHOT_LIST_DATA, SCREENSHOT_SYNC_RESULT, ShellMessage,
};
use crate::state::{self, DeviceSummary, ScreenshotTransfer};
use crate::ui;

const TARGET_APP_NAME_KEYWORD: &str = "Shell++";
const CANDIDATE_PKG_NAMES: [&str; 1] = [QUICK_APP_PACKAGE];
const QUICK_APP_ENTRY_PAGE: &str = "pages/index";
const DESKTOP_SAVE_DIR: &str = "";
const MOBILE_SAVE_DIR: &str = "Pictures/Shell++";

/// 从 AstroBox 事件 payload 中提取真正的 interconnect 文本。
/// 参考 Varclass 插件做法，兼容宿主可能包一层 payloadText / payload 的情况。
pub fn extract_payload_text(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<Value>(payload) {
        if let Some(text) = json.get("payloadText").and_then(|value| value.as_str()) {
            return text.to_string();
        }
        if let Some(payload_value) = json.get("payload") {
            if let Some(text) = payload_value.as_str() {
                return text.to_string();
            }
            return payload_value.to_string();
        }
    }
    payload.to_string()
}

fn brief(text: &str, max_len: usize) -> String {
    let mut output = text.replace('\n', " ");
    if output.len() > max_len {
        output.truncate(max_len);
        output.push_str("...");
    }
    output
}

/// 优先获取已连接设备；没有在线设备时回退到历史设备列表，方便先预注册通道。
pub async fn refresh_devices() -> String {
    state::append_log("refresh_devices: 开始读取设备列表");
    let connected = device::get_connected_device_list().await;
    let selected = if let Some(device) = connected.first() {
        state::append_log(format!(
            "refresh_devices: 已连接设备 {} {}",
            device.name, device.addr
        ));
        Some((device.name.clone(), device.addr.clone(), true))
    } else {
        state::append_warn("refresh_devices: 未发现已连接设备，回退历史设备列表");
        let all = device::get_device_list().await;
        all.first().map(|device| {
            state::append_log(format!(
                "refresh_devices: 历史设备 {} {}",
                device.name, device.addr
            ));
            (device.name.clone(), device.addr.clone(), false)
        })
    };

    state::with_state(|state| {
        let new_addr = selected.as_ref().map(|(_, addr, _)| addr.as_str());
        let old_addr = state.selected_device.as_ref().map(|d| d.addr.as_str());
        let device_changed = new_addr != old_addr;

        state.selected_device = selected.as_ref().map(|(name, addr, _)| DeviceSummary {
            name: name.clone(),
            addr: addr.clone(),
        });
        if device_changed {
            state.registered_recv = false;
        }
        state.connected = selected
            .as_ref()
            .map(|(_, _, connected)| *connected)
            .unwrap_or(false);
        state.last_status = match selected {
            Some((name, _, true)) => format!("发现已连接设备：{}", name),
            Some((name, _, false)) => format!("发现历史设备：{}，请先在 AstroBox 连接它", name),
            None => "没有发现设备，请先在 AstroBox 连接手表，并授予 device 权限".to_string(),
        };
    });

    "refresh-ok".to_string()
}

async fn first_connected_device() -> Result<DeviceSummary, String> {
    state::append_log("first_connected_device: 读取已连接设备");
    let connected = device::get_connected_device_list().await;
    if let Some(device) = connected.first() {
        let summary = DeviceSummary {
            name: device.name.clone(),
            addr: device.addr.clone(),
        };
        let device_changed = state::with_state(|state| {
            let changed = state
                .selected_device
                .as_ref()
                .map(|d| d.addr != summary.addr)
                .unwrap_or(true);
            state.selected_device = Some(summary.clone());
            state.connected = true;
            if changed {
                state.registered_recv = false;
            }
            changed
        });
        if device_changed {
            state::append_log(format!(
                "first_connected_device: 设备已变更，重置注册状态 {} {}",
                summary.name, summary.addr
            ));
        }
        state::append_log(format!(
            "first_connected_device: 使用 {} {}",
            summary.name, summary.addr
        ));
        return Ok(summary);
    }

    refresh_devices().await;
    Err("无已连接设备，请先在 AstroBox 连接手表".to_string())
}

async fn first_bootstrap_device() -> Result<(DeviceSummary, bool), String> {
    state::append_log("bootstrap_device: 读取可订阅设备");
    let connected = device::get_connected_device_list().await;
    if let Some(device) = connected.first() {
        return Ok((
            DeviceSummary {
                name: device.name.clone(),
                addr: device.addr.clone(),
            },
            true,
        ));
    }

    let all = device::get_device_list().await;
    all.first()
        .map(|device| {
            (
                DeviceSummary {
                    name: device.name.clone(),
                    addr: device.addr.clone(),
                },
                false,
            )
        })
        .ok_or_else(|| "未检测到可用设备，请先连接或扫描手表".to_string())
}

async fn resolve_target_app_info(device: &DeviceSummary) -> Result<thirdpartyapp::AppInfo, String> {
    state::append_log(format!(
        "resolve_pkg: 读取设备应用列表 addr={}",
        device.addr
    ));
    let apps = thirdpartyapp::get_thirdparty_app_list(&device.addr)
        .await
        .map_err(|error| {
            format!(
                "读取手表应用列表失败，请授予 thirdpartyapp 权限: {:?}",
                error
            )
        })?;

    state::append_log(format!("resolve_pkg: 收到 {} 个应用", apps.len()));

    if let Some(app) = apps
        .iter()
        .find(|app| app.app_name.contains(TARGET_APP_NAME_KEYWORD))
    {
        state::append_log(format!(
            "resolve_pkg: 按名称匹配 {} ({})",
            app.app_name, app.package_name
        ));
        return Ok(app.clone());
    }

    if let Some(app) = apps.iter().find(|app| {
        CANDIDATE_PKG_NAMES
            .iter()
            .any(|pkg_name| *pkg_name == app.package_name)
    }) {
        state::append_log(format!(
            "resolve_pkg: 按包名匹配 {} ({})",
            app.app_name, app.package_name
        ));
        return Ok(app.clone());
    }

    let app_names = apps
        .iter()
        .map(|app| format!("{}({})", app.app_name, app.package_name))
        .collect::<Vec<_>>()
        .join(", ");

    Err(format!(
        "手表未找到 Shell++ 快应用。当前应用: {}",
        brief(&app_names, 180)
    ))
}

async fn resolve_target_pkg_name(device: &DeviceSummary) -> Result<String, String> {
    resolve_target_app_info(device)
        .await
        .map(|app_info| app_info.package_name)
}

/// 注册 Shell++ 快应用消息接收；重复注册按成功处理。
pub async fn ensure_interconnect_registered(
    device: &DeviceSummary,
    pkg_name: &str,
) -> Result<(), String> {
    let snapshot = state::snapshot();
    if snapshot.registered_recv && snapshot.target_pkg_name == pkg_name {
        return Ok(());
    }

    state::append_log(format!(
        "register_recv: addr={} pkg={}",
        device.addr, pkg_name
    ));
    match register::register_interconnect_recv(&device.addr, pkg_name).await {
        Ok(()) => {
            mark_registered(pkg_name);
            Ok(())
        }
        Err(error) => {
            let raw = format!("{:?}", error);
            let lower = raw.to_lowercase();
            if lower.contains("already") || lower.contains("exists") || lower.contains("duplicate")
            {
                mark_registered(pkg_name);
                Ok(())
            } else {
                Err(format!(
                    "注册 Shell++ 接收失败，请授予 register_interconnect_recv 权限(addr={}, pkg={}): {}",
                    device.addr, pkg_name, raw
                ))
            }
        }
    }
}

fn mark_registered(pkg_name: &str) {
    state::append_log(format!("register_recv: 成功 pkg={}", pkg_name));
    state::with_state(|state| {
        state.registered_recv = true;
        state.target_pkg_name = pkg_name.to_string();
        state.last_status = "已注册 Shell++ 消息接收".to_string();
    });
}

async fn send_to_quick_app(
    device: &DeviceSummary,
    pkg_name: &str,
    data: String,
) -> Result<(), String> {
    state::append_log(format!(
        "send: addr={} pkg={} payload={}",
        device.addr,
        pkg_name,
        brief(&data, 120)
    ));
    interconnect::send_qaic_message(&device.addr, pkg_name, &data)
        .await
        .map_err(|error| {
            format!(
                "发送到 Shell++ 快应用失败，请授予 interconnect 权限并在手表打开 Shell++: {:?}",
                error
            )
        })
}

fn sanitize_file_name(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            output.push(ch);
        } else {
            output.push('_');
        }
    }
    let trimmed = output.trim_matches('_');
    if trimmed.is_empty() {
        "screenshot".to_string()
    } else {
        trimmed.to_string()
    }
}

fn screenshot_file_name(shot_id: &str) -> String {
    file_name_with_extension(shot_id, "png")
}

fn raw_file_name(shot_id: &str) -> String {
    file_name_with_extension(shot_id, "raw")
}

fn file_name_with_extension(shot_id: &str, extension: &str) -> String {
    let base = sanitize_file_name(shot_id);
    let suffix = format!(".{}", extension);
    if base.to_ascii_lowercase().ends_with(&suffix) {
        base
    } else {
        format!("{}{}", base, suffix)
    }
}

fn current_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn normalize_http_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    }
}

fn is_mobile_platform(platform: &str) -> bool {
    let lower = platform.to_lowercase();
    lower.contains("android")
        || lower.contains("ios")
        || lower.contains("iphone")
        || lower.contains("ipad")
        || lower.contains("mobile")
}

fn platform_save_mode(platform: &str) -> (&'static str, &'static str) {
    if is_mobile_platform(platform) {
        ("手机保存到相册", MOBILE_SAVE_DIR)
    } else {
        ("电脑手动选择路径", DESKTOP_SAVE_DIR)
    }
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0usize;
    let mut padding = 0usize;

    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                0
            }
            _ => return Err(format!("非法 base64 字符: {}", byte)),
        };

        chunk[chunk_len] = value;
        chunk_len += 1;

        if chunk_len == 4 {
            output.push((chunk[0] << 2) | (chunk[1] >> 4));
            if padding < 2 {
                output.push((chunk[1] << 4) | (chunk[2] >> 2));
            }
            if padding < 1 {
                output.push((chunk[2] << 6) | chunk[3]);
            }
            chunk_len = 0;
            padding = 0;
        }
    }

    if chunk_len != 0 {
        return Err("base64 长度不完整".to_string());
    }

    Ok(output)
}

fn send_chunk_ack_sync(session_id: &str, phase: &str, index: i64, ok: bool) {
    let snapshot = state::snapshot();
    let Some(device) = snapshot.selected_device else {
        state::append_warn("chunk_ack: 无已选设备，无法回复 ACK");
        return;
    };
    let payload = protocol::build_screenshot_chunk_ack(session_id, phase, index, ok);
    if let Err(error) = astrobox_ng_wit::block_on(async {
        send_to_quick_app(&device, &snapshot.target_pkg_name, payload).await
    }) {
        state::append_warn(format!("chunk_ack: 发送失败 {}", error));
    }
}

/// 启动时自动尝试订阅首个设备的 Shell++ 通道。
pub async fn bootstrap() -> Result<(), String> {
    state::append_log("bootstrap: 开始订阅 Shell++ 通道");
    let (device, connected) = first_bootstrap_device().await?;
    let pkg_name = if connected {
        resolve_target_pkg_name(&device)
            .await
            .unwrap_or_else(|error| {
                state::append_warn(format!("bootstrap: 解析包名失败，回退默认包名: {}", error));
                QUICK_APP_PACKAGE.to_string()
            })
    } else {
        QUICK_APP_PACKAGE.to_string()
    };
    ensure_interconnect_registered(&device, &pkg_name).await?;
    state::with_state(|state| {
        state.selected_device = Some(device.clone());
        state.connected = connected;
        state.target_pkg_name = pkg_name.clone();
        state.last_status = if connected {
            format!("已订阅 Shell++ 通道：{} ({})", device.addr, pkg_name)
        } else {
            format!(
                "已预注册 Shell++ 通道：{} ({})，待连接后生效",
                device.addr, pkg_name
            )
        };
    });
    Ok(())
}

/// 设备后连场景下自动重试订阅；已订阅时直接返回。
/// 参考 Varclass 的 bootstrap_if_needed 实现。
pub async fn bootstrap_if_needed() -> Result<(), String> {
    let already_registered = state::snapshot().registered_recv;
    if already_registered {
        return Ok(());
    }

    match bootstrap().await {
        Ok(()) => Ok(()),
        Err(err) => {
            state::with_state(|state| {
                state.registered_recv = false;
                state.last_status = format!("等待设备连接后自动重试: {}", err);
            });
            Err(err)
        }
    }
}

pub async fn handshake() -> String {
    match try_handshake().await {
        Ok(()) => "handshake-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.last_status = message.clone());
            message
        }
    }
}

pub async fn launch_quick_app() -> String {
    match try_launch_quick_app().await {
        Ok(()) => "launch-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.last_status = message.clone());
            message
        }
    }
}

async fn try_launch_quick_app() -> Result<(), String> {
    let device = first_connected_device().await?;
    let app_info = resolve_target_app_info(&device).await?;
    let pkg_name = app_info.package_name.clone();
    state::append_log(format!(
        "launch_qa: addr={} pkg={} page={}",
        device.addr, pkg_name, QUICK_APP_ENTRY_PAGE
    ));
    thirdpartyapp::launch_qa(&device.addr, &app_info, QUICK_APP_ENTRY_PAGE)
        .await
        .map_err(|error| {
            format!(
                "打开 Shell++ 失败，请授予 thirdpartyapp 权限并确认手表已连接: {:?}",
                error
            )
        })?;
    state::with_state(|state| {
        state.target_pkg_name = pkg_name;
        state.connected = true;
        state.last_status = "已请求打开 Shell++ 快应用".to_string();
    });
    Ok(())
}

async fn try_handshake() -> Result<(), String> {
    bootstrap_if_needed().await?;
    let device = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&device).await?;
    ensure_interconnect_registered(&device, &pkg_name).await?;
    send_to_quick_app(&device, &pkg_name, protocol::build_handshake(0)).await?;
    state::with_state(|state| {
        state.target_pkg_name = pkg_name;
        state.last_status = "已发送握手，等待 Shell++ 回复".to_string();
    });
    Ok(())
}

pub async fn exec_terminal_input() -> String {
    let cmd = state::snapshot().terminal_input.trim().to_string();
    exec_command(&cmd).await
}

pub async fn exec_command(cmd: &str) -> String {
    exec_command_with_callback(cmd, "").await
}

pub async fn exec_command_with_callback(cmd: &str, callback: &str) -> String {
    let cmd = cmd.trim();
    state::append_log(format!(
        "[CLI] 准备执行命令：bytes={}, callback={}",
        cmd.len(),
        if callback.is_empty() {
            "none"
        } else {
            "loopback"
        }
    ));
    if cmd.is_empty() {
        state::append_warn("[CLI] 命令为空，已拒绝");
        return "命令不能为空".to_string();
    }
    if cmd.len() > 2048 {
        return "命令过长，最多 2048 字节".to_string();
    }
    if !callback.is_empty() && !is_loopback_callback(callback) {
        state::append_warn("[CLI] callback 不是允许的 loopback 地址，已拒绝");
        return "callback 仅允许 http://127.0.0.1 或 http://localhost".to_string();
    }
    match try_exec_command(cmd.to_string(), callback.to_string()).await {
        Ok(()) => "exec-command-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.terminal_status = message.clone());
            message
        }
    }
}

pub async fn prompt_and_exec_command() -> String {
    let result = dialog::show_dialog(
        dialog::DialogType::Input,
        dialog::DialogStyle::Website,
        &dialog::DialogInfo {
            title: "执行终端命令".into(),
            content: "请输入要在手表端执行的命令".into(),
            buttons: vec![
                dialog::DialogButton {
                    id: "ok".into(),
                    primary: true,
                    content: "执行".into(),
                },
                dialog::DialogButton {
                    id: "cancel".into(),
                    primary: false,
                    content: "取消".into(),
                },
            ],
        },
    )
    .await;
    if result.clicked_btn_id != "ok" {
        return "已取消执行命令".to_string();
    }
    let cmd = result.input_result.trim().to_string();
    if cmd.is_empty() {
        return "命令不能为空".to_string();
    }
    match try_exec_command(cmd, String::new()).await {
        Ok(()) => "exec-command-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.terminal_status = message.clone());
            message
        }
    }
}

async fn try_exec_command(cmd: String, callback: String) -> Result<(), String> {
    state::append_log("[CLI] 正在初始化设备与 Interconnect");
    bootstrap_if_needed().await?;
    let device = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&device).await?;
    ensure_interconnect_registered(&device, &pkg_name).await?;
    let req_id = format!("abv2-{}", current_millis());
    state::with_state(|state| {
        state.active_panel = "terminal".to_string();
        state.target_pkg_name = pkg_name.clone();
        state.pending_exec_req_id = req_id.clone();
        state.pending_cli_callback = callback;
        state.terminal_last_command = cmd.clone();
        state.terminal_status = "已发送命令，等待 QuickApp 接收".to_string();
        state.terminal_output = format!("> {}\n\n等待返回...", cmd);
        state.last_status = "已发送终端命令".to_string();
    });
    let result = send_to_quick_app(
        &device,
        &pkg_name,
        protocol::build_exec_command(&req_id, &cmd),
    )
    .await;
    match &result {
        Ok(()) => state::append_log(format!("[CLI] execCommand 已投递：req_id={}", req_id)),
        Err(error) => state::append_warn(format!("[CLI] execCommand 投递失败：{}", error)),
    }
    result
}

pub async fn request_screenshot_list() -> String {
    match try_request_screenshot_list().await {
        Ok(()) => "request-list-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.last_status = message.clone());
            message
        }
    }
}

pub async fn request_raw_list() -> String {
    match try_request_raw_list().await {
        Ok(()) => "raw-list-request-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.last_status = message.clone());
            message
        }
    }
}

async fn try_request_screenshot_list() -> Result<(), String> {
    bootstrap_if_needed().await?;
    let device = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&device).await?;
    ensure_interconnect_registered(&device, &pkg_name).await?;
    if !state::snapshot().connected {
        send_to_quick_app(&device, &pkg_name, protocol::build_handshake(0)).await?;
    }
    send_to_quick_app(
        &device,
        &pkg_name,
        protocol::build_type_message(REQUEST_SCREENSHOT_LIST),
    )
    .await?;
    state::with_state(|state| {
        state.target_pkg_name = pkg_name;
        state.last_status = "已请求截图列表".to_string();
    });
    Ok(())
}

async fn try_request_raw_list() -> Result<(), String> {
    bootstrap_if_needed().await?;
    let device = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&device).await?;
    ensure_interconnect_registered(&device, &pkg_name).await?;
    send_to_quick_app(
        &device,
        &pkg_name,
        protocol::build_type_message(REQUEST_RAW_LIST),
    )
    .await?;
    state::with_state(|state| {
        state.active_panel = "debug".to_string();
        state.target_pkg_name = pkg_name;
        state.last_status = "已请求 RAW 文件列表".to_string();
    });
    Ok(())
}

pub fn enter_screenshot_selection() -> String {
    let snapshot = state::snapshot();
    if snapshot.screenshots.is_empty() {
        let message = "暂无截图列表，请先点击“拉取截图列表”".to_string();
        state::with_state(|state| state.last_status = message.clone());
        return message;
    }
    state::with_state(|state| {
        state.selecting_screenshots = true;
        state.last_status = "请选择要同步的截图，可一键全选".to_string();
    });
    "selection-updated".to_string()
}

pub fn cancel_screenshot_selection() -> String {
    state::with_state(|state| {
        state.selecting_screenshots = false;
        state.selected_shot_ids.clear();
        state.last_status = "已取消截图选择".to_string();
    });
    "selection-updated".to_string()
}

pub fn toggle_select_all_screenshots() -> String {
    state::with_state(|state| {
        let all_ids = state
            .screenshots
            .iter()
            .filter(|item| !item.shot_id.is_empty())
            .map(|item| item.shot_id.clone())
            .collect::<Vec<_>>();
        if !all_ids.is_empty() && state.selected_shot_ids.len() == all_ids.len() {
            state.selected_shot_ids.clear();
            state.last_status = "已取消全选".to_string();
        } else {
            state.selected_shot_ids = all_ids;
            state.last_status = format!("已全选 {} 张截图", state.selected_shot_ids.len());
        }
        state.selecting_screenshots = true;
    });
    "selection-updated".to_string()
}

pub fn toggle_screenshot_selection(index: usize) -> String {
    state::with_state(|state| {
        let Some(item) = state.screenshots.get(index) else {
            state.last_status = "截图索引不存在，请重新拉取截图列表".to_string();
            return;
        };
        if item.shot_id.is_empty() {
            state.last_status = "截图 ID 为空，无法选择".to_string();
            return;
        }
        if let Some(pos) = state
            .selected_shot_ids
            .iter()
            .position(|shot_id| shot_id == &item.shot_id)
        {
            state.selected_shot_ids.remove(pos);
            state.last_status = format!("已取消选择 {}", item.shot_id);
        } else {
            state.selected_shot_ids.push(item.shot_id.clone());
            state.last_status = format!("已选择 {}", item.shot_id);
        }
        state.selecting_screenshots = true;
    });
    "selection-updated".to_string()
}

pub fn toggle_transfer_mode() -> String {
    state::with_state(|state| {
        if state.sync_mode == "fetch" {
            state.sync_mode = "interconnect".to_string();
            state.last_status = "已切换为 Interconnect 保存模式".to_string();
        } else {
            state.sync_mode = "fetch".to_string();
            state.last_status = "已切换为 Fetch 直传模式".to_string();
        }
    });
    "mode-updated".to_string()
}

pub async fn set_fetch_url() -> String {
    let current = state::snapshot().fetch_url;
    let result = dialog::show_dialog(
        dialog::DialogType::Input,
        dialog::DialogStyle::Website,
        &dialog::DialogInfo {
            title: "设置 Fetch URL".into(),
            content: if current.is_empty() {
                "请输入接收端地址，例如 192.168.1.100:8765".into()
            } else {
                current.clone()
            },
            buttons: vec![
                dialog::DialogButton {
                    id: "ok".into(),
                    primary: true,
                    content: "确定".into(),
                },
                dialog::DialogButton {
                    id: "cancel".into(),
                    primary: false,
                    content: "取消".into(),
                },
            ],
        },
    )
    .await;
    if result.clicked_btn_id != "ok" {
        return "已取消设置 Fetch URL".to_string();
    }
    let url = normalize_http_url(&result.input_result);
    if url.is_empty() {
        return "Fetch URL 不能为空".to_string();
    }
    state::with_state(|state| {
        state.fetch_url = url.clone();
        state.last_status = format!("已设置 Fetch URL：{}", url);
    });
    "mode-updated".to_string()
}

pub async fn start_selected_screenshots() -> String {
    let snapshot = state::snapshot();
    if snapshot.active_transfer.is_some() {
        return "已有截图正在同步，请等待完成".to_string();
    }
    let selected = snapshot
        .screenshots
        .iter()
        .filter(|item| {
            !item.shot_id.is_empty()
                && snapshot
                    .selected_shot_ids
                    .iter()
                    .any(|shot_id| shot_id == &item.shot_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return "请先选择要同步的截图".to_string();
    }

    state::with_state(|state| {
        state.selecting_screenshots = false;
        state.sync_total = selected.len();
        state.sync_done = 0;
        state.sync_failed = 0;
        state.sync_queue.clear();
        state.last_status = format!("准备批量同步 {} 张截图", selected.len());
    });

    if snapshot.sync_mode == "fetch" {
        match try_start_fetch_sync(selected).await {
            Ok(()) => "sync-request-sent".to_string(),
            Err(message) => {
                state::with_state(|state| state.last_status = message.clone());
                message
            }
        }
    } else {
        state::with_state(|state| {
            state.sync_queue = selected;
        });
        match start_next_interconnect_screenshot().await {
            Ok(()) => "sync-request-sent".to_string(),
            Err(message) => {
                state::with_state(|state| state.last_status = message.clone());
                message
            }
        }
    }
}

async fn try_start_fetch_sync(
    screenshots: Vec<crate::protocol::ScreenshotItem>,
) -> Result<(), String> {
    let snapshot = state::snapshot();
    let url = normalize_http_url(&snapshot.fetch_url);
    if url.is_empty() {
        return Err("请先设置 Fetch URL".to_string());
    }
    bootstrap_if_needed().await?;
    let device = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&device).await?;
    ensure_interconnect_registered(&device, &pkg_name).await?;
    let session_id = format!("fetch-{}", current_millis());
    state::with_state(|state| {
        state.fetch_url = url.clone();
        state.target_pkg_name = pkg_name.clone();
        state.active_transfer = Some(ScreenshotTransfer {
            source_session_id: session_id.clone(),
            save_session_id: None,
            shot_id: "Fetch 批量同步".to_string(),
            file_name: String::new(),
            total: screenshots.len() as i64,
            received: 0,
            received_bytes: 0,
            size: 0,
            platform: String::new(),
            mode_label: "Fetch 直传".to_string(),
            started_at_ms: current_millis(),
            rate_kbps: 0.0,
        });
        state.last_status = format!("已请求手表用 Fetch 同步 {} 张截图", screenshots.len());
    });
    send_to_quick_app(
        &device,
        &pkg_name,
        protocol::build_fetch_sync_request(&session_id, &url, &screenshots),
    )
    .await
}

async fn start_next_interconnect_screenshot() -> Result<(), String> {
    let next = state::with_state(|state| {
        if state.sync_queue.is_empty() {
            None
        } else {
            Some(state.sync_queue.remove(0))
        }
    });
    let Some(item) = next else {
        let snapshot = state::snapshot();
        state::with_state(|state| {
            state.active_transfer = None;
            state.last_status = format!(
                "批量同步完成：成功 {}，失败 {}",
                snapshot.sync_done, snapshot.sync_failed
            );
        });
        return Ok(());
    };
    try_start_interconnect_screenshot(item).await
}

async fn try_start_interconnect_screenshot(
    item: crate::protocol::ScreenshotItem,
) -> Result<(), String> {
    try_start_interconnect_file(
        item,
        "截图",
        "png",
        screenshot_file_name,
        protocol::build_request_screenshot_data,
    )
    .await
}

async fn try_start_interconnect_raw(item: crate::protocol::ScreenshotItem) -> Result<(), String> {
    try_start_interconnect_file(
        item,
        "RAW",
        "raw",
        raw_file_name,
        protocol::build_request_raw_data,
    )
    .await
}

async fn try_start_interconnect_file(
    item: crate::protocol::ScreenshotItem,
    file_kind: &str,
    extension: &str,
    file_name_builder: fn(&str) -> String,
    request_builder: fn(&str) -> String,
) -> Result<(), String> {
    if item.shot_id.is_empty() {
        return Err(format!("{} ID 为空，无法同步", file_kind));
    }
    if state::snapshot().active_transfer.is_some() {
        return Err("已有文件正在同步，请等待完成".to_string());
    }

    bootstrap_if_needed().await?;
    let device = first_connected_device().await?;
    let pkg_name = resolve_target_pkg_name(&device).await?;
    ensure_interconnect_registered(&device, &pkg_name).await?;

    let platform = os::platform().await;
    let (save_mode_label, default_directory) = platform_save_mode(&platform);
    let mode_label = if file_kind == "RAW" {
        format!("{} RAW", save_mode_label)
    } else {
        save_mode_label.to_string()
    };
    let file_name = file_name_builder(&item.shot_id);
    state::append_log(format!(
        "sync_file: kind={} platform={} mode={} file={}",
        file_kind, platform, mode_label, file_name
    ));
    state::with_state(|state| {
        state.host_platform = platform.clone();
        state.target_pkg_name = pkg_name.clone();
        state.last_status = format!("准备保存 {}：{}", mode_label, file_name);
    });
    ui::rerender_if_possible();

    let save_session = dialog::save_file_start(&dialog::FilterConfig {
        multiple: false,
        extensions: vec![extension.into()],
        default_directory: default_directory.into(),
        default_file_name: file_name.clone(),
    })
    .await
    .map_err(|_| {
        if is_mobile_platform(&platform) {
            format!("保存 {} 到相册/图片目录失败或已取消", file_kind)
        } else {
            "已取消选择保存路径".to_string()
        }
    })?;

    state::with_state(|state| {
        state.active_transfer = Some(ScreenshotTransfer {
            source_session_id: String::new(),
            save_session_id: Some(save_session.session_id),
            shot_id: item.shot_id.clone(),
            file_name: save_session.name.clone(),
            total: 0,
            received: 0,
            received_bytes: 0,
            size: 0,
            platform: platform.clone(),
            mode_label: mode_label.clone(),
            started_at_ms: current_millis(),
            rate_kbps: 0.0,
        });
        state.last_status = format!("已创建保存会话，正在请求手表发送 {}", item.shot_id);
    });
    ui::rerender_if_possible();

    if let Err(error) = send_to_quick_app(&device, &pkg_name, request_builder(&item.shot_id)).await
    {
        dialog::save_file_abort(save_session.session_id).await;
        state::with_state(|state| {
            state.active_transfer = None;
        });
        return Err(error);
    }

    Ok(())
}

pub async fn sync_latest_raw() -> String {
    let raw_item = state::snapshot()
        .screenshots
        .into_iter()
        .filter(is_raw_screenshot_item)
        .max_by_key(|item| item.captured_at_unix.max(item.index));
    let Some(item) = raw_item else {
        let _ = try_request_raw_list().await;
        return "暂无 RAW 文件，已请求 RAW 列表".to_string();
    };
    state::with_state(|state| {
        state.active_panel = "debug".to_string();
        state.sync_total = 1;
        state.sync_done = 0;
        state.sync_failed = 0;
        state.last_status = format!("准备同步 RAW：{}", item.shot_id);
    });
    match try_start_interconnect_raw(item).await {
        Ok(()) => "raw-sync-sent".to_string(),
        Err(message) => {
            state::with_state(|state| state.last_status = message.clone());
            message
        }
    }
}

fn is_raw_screenshot_item(item: &crate::protocol::ScreenshotItem) -> bool {
    let value = item.shot_id.to_ascii_lowercase();
    let source = item.source.to_ascii_lowercase();
    source == "framebuffer_raw"
        || value.ends_with(".raw")
        || value.contains(".raw#")
        || value.contains("_raw")
}

pub async fn handle_interconnect_message(event_payload: String) -> String {
    handle_interconnect_message_sync(&event_payload)
}

pub fn handle_interconnect_message_sync(event_payload: &str) -> String {
    let payload_text = extract_payload_text(event_payload);
    state::append_log(format!("recv: {}", brief(&payload_text, 160)));
    let message = match serde_json::from_str::<ShellMessage>(&payload_text) {
        Ok(message) => message,
        Err(error) => {
            state::with_state(|state| {
                state.last_message = format!("无法解析消息：{}", error);
            });
            ui::rerender_if_possible();
            return "parse-error".to_string();
        }
    };

    match message.message_type.as_str() {
        HANDSHAKE => handle_handshake_message_sync(message),
        HEARTBEAT => handle_heartbeat_message_sync(),
        HEARTBEAT_ACK => {
            state::with_state(|state| {
                state.connected = true;
                state.last_status = "收到心跳确认".to_string();
                state.last_message = "heartbeatAck".to_string();
            });
            ui::rerender_if_possible();
            "heartbeat-ack-ok".to_string()
        }
        SCREENSHOT_LIST_DATA => {
            let count = message.screenshots.len();
            state::with_state(|state| {
                state.screenshots = message.screenshots;
                state.last_status = format!("收到截图列表：{} 张", count);
                state.last_message = "screenshotListData".to_string();
            });
            ui::rerender_if_possible();
            "screenshot-list-ok".to_string()
        }
        RAW_LIST_DATA => {
            let count = message.screenshots.len();
            state::with_state(|state| {
                state.active_panel = "debug".to_string();
                state.screenshots = message.screenshots;
                state.last_status = format!("收到 RAW 列表：{} 个", count);
                state.last_message = "rawListData".to_string();
            });
            ui::rerender_if_possible();
            "raw-list-ok".to_string()
        }
        SCREENSHOT_CHUNK_START => handle_screenshot_chunk_start_sync(message),
        SCREENSHOT_CHUNK_PART => handle_screenshot_chunk_part_sync(message),
        SCREENSHOT_CHUNK_FINISH => handle_screenshot_chunk_finish_sync(message),
        SCREENSHOT_SYNC_RESULT => handle_screenshot_sync_result_sync(message),
        SCREENSHOT_FETCH_PROGRESS => handle_screenshot_fetch_progress_sync(message),
        SCREENSHOT_FETCH_RESULT => handle_screenshot_fetch_result_sync(message),
        EXEC_ACK => handle_exec_ack_sync(message),
        EXEC_RESULT => handle_exec_result_sync(message),
        other => {
            state::with_state(|state| {
                state.last_message = format!("收到消息：{}", other);
            });
            ui::rerender_if_possible();
            "message-ok".to_string()
        }
    }
}

fn handle_exec_ack_sync(message: ShellMessage) -> String {
    state::append_log(format!(
        "[CLI] 收到 execAck：req_id={}, accepted={}",
        message.req_id, message.accepted
    ));
    state::with_state(|state| {
        state.active_panel = "terminal".to_string();
        if !state.pending_exec_req_id.is_empty() && state.pending_exec_req_id != message.req_id {
            state.last_message = format!("忽略过期 execAck：{}", message.req_id);
            return;
        }
        if message.accepted {
            state.terminal_status = "QuickApp 已接收命令，等待 Lua 返回".to_string();
        } else {
            state.pending_exec_req_id.clear();
            let reason = if message.reason.is_empty() {
                "unknown".to_string()
            } else {
                message.reason
            };
            state.terminal_status = format!("命令被拒绝：{}", reason);
            state.terminal_output = format!(
                "> {}\n\n{}",
                state.terminal_last_command, state.terminal_status
            );
        }
        state.last_status = state.terminal_status.clone();
        state.last_message = "execAck".to_string();
    });
    ui::rerender_if_possible();
    "exec-ack-ok".to_string()
}

fn handle_exec_result_sync(message: ShellMessage) -> String {
    state::append_log(format!(
        "[CLI] 收到 execResult：req_id={}, stdout_bytes={}, stderr_bytes={}, exitcode={:?}, timeout={}",
        message.req_id,
        message.stdout.len(),
        message.stderr.len(),
        message.exitcode,
        message.timed_out
    ));
    let callback_payload = serde_json::json!({
        "reqId": message.req_id,
        "cmd": message.cmd,
        "stdout": message.stdout,
        "stderr": message.stderr,
        "exitcode": message.exitcode,
        "timedOut": message.timed_out
    })
    .to_string();
    let callback = state::snapshot().pending_cli_callback;
    state::with_state(|state| {
        state.active_panel = "terminal".to_string();
        if !state.pending_exec_req_id.is_empty() && state.pending_exec_req_id != message.req_id {
            state.last_message = format!("忽略过期 execResult：{}", message.req_id);
            return;
        }
        state.pending_exec_req_id.clear();
        state.pending_cli_callback.clear();
        let cmd = if message.cmd.is_empty() {
            state.terminal_last_command.clone()
        } else {
            message.cmd.clone()
        };
        state.terminal_last_command = cmd.clone();
        state.terminal_status = if message.timed_out {
            "命令等待超时".to_string()
        } else {
            format!(
                "命令完成，exitcode={}",
                message.exitcode.unwrap_or_default()
            )
        };
        state.terminal_output = format_terminal_output(
            &cmd,
            &message.stdout,
            &message.stderr,
            message.exitcode,
            message.timed_out,
        );
        state.last_status = state.terminal_status.clone();
        state.last_message = "execResult".to_string();
    });
    if !callback.is_empty() {
        let separator = if callback.contains('?') { "&" } else { "?" };
        let url = format!(
            "{}{}response={}",
            callback,
            separator,
            percent_encode(&callback_payload)
        );
        state::append_log("[CLI] 正在通过 open-url 触发 loopback 回调");
        dialog::open_url(&url);
        state::append_log("[CLI] open-url 调用已提交");
    } else {
        state::append_warn("[CLI] execResult 没有关联 callback，仅更新终端界面");
    }
    ui::rerender_if_possible();
    "exec-result-ok".to_string()
}

fn is_loopback_callback(value: &str) -> bool {
    value.starts_with("http://127.0.0.1:") || value.starts_with("http://localhost:")
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{:02X}", byte));
        }
    }
    output
}

fn format_terminal_output(
    cmd: &str,
    stdout: &str,
    stderr: &str,
    exitcode: Option<i64>,
    timed_out: bool,
) -> String {
    let stdout_text = normalize_terminal_section(stdout);
    let stderr_text = normalize_terminal_section(stderr);
    let mut status = format!("exitcode: {}", exitcode.unwrap_or_default());
    if timed_out {
        status.push_str(" · timeout");
    }
    format!(
        "命令\n------\n{}\n------\n输出\n------\n{}\n------\n错误\n------\n{}\n------\n{}",
        cmd.trim(),
        stdout_text,
        stderr_text,
        status
    )
}

fn normalize_terminal_section(value: &str) -> String {
    let trimmed = value.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        "(empty)".to_string()
    } else {
        trimmed.to_string()
    }
}

fn handle_handshake_message_sync(message: ShellMessage) -> String {
    let count = message.count.unwrap_or(-1);
    state::with_state(|state| {
        state.last_status = format!("握手回复 count={}", count);
        state.last_message = "handshake".to_string();
    });

    if (0..2).contains(&count) {
        let snapshot = state::snapshot();
        if let Some(device) = snapshot.selected_device {
            let reply = protocol::build_handshake(count + 1);
            match astrobox_ng_wit::block_on(async {
                send_to_quick_app(&device, &snapshot.target_pkg_name, reply).await
            }) {
                Ok(()) => {
                    state::with_state(|state| {
                        state.connected = count > 0;
                    });
                    state::append_log(format!("握手回复成功 count={}", count + 1));
                }
                Err(err) => {
                    state::with_state(|state| {
                        state.connected = false;
                    });
                    state::append_warn(format!("握手回复失败: {}", err));
                }
            }
        } else {
            state::with_state(|state| {
                state.connected = false;
            });
            state::append_warn("收到握手但无已选设备，无法回复");
        }
    } else {
        state::with_state(|state| {
            state.connected = count > 0;
        });
    }

    ui::rerender_if_possible();
    "handshake-ok".to_string()
}

fn handle_heartbeat_message_sync() -> String {
    let snapshot = state::snapshot();
    let ack_result = if let Some(device) = snapshot.selected_device {
        astrobox_ng_wit::block_on(async {
            send_to_quick_app(
                &device,
                &snapshot.target_pkg_name,
                protocol::build_heartbeat_ack(),
            )
            .await
        })
    } else {
        Err("无已选设备".to_string())
    };

    match ack_result {
        Ok(()) => {
            state::with_state(|state| {
                state.connected = true;
                state.last_status = "收到心跳，已回复".to_string();
                state.last_message = "heartbeat".to_string();
            });
        }
        Err(err) => {
            state::append_warn(format!("心跳 ack 发送失败: {}", err));
            state::with_state(|state| {
                state.connected = false;
                state.last_status = format!("心跳 ack 发送失败: {}", err);
                state.last_message = "heartbeat-ack-failed".to_string();
            });
        }
    }

    ui::rerender_if_possible();
    "heartbeat-ok".to_string()
}

fn handle_screenshot_sync_result_sync(message: ShellMessage) -> String {
    if message.success {
        return "screenshot-sync-result-ok".to_string();
    }
    let reason = if message.reason.is_empty() {
        "unknown".to_string()
    } else {
        message.reason
    };
    let active = state::with_state(|state| state.active_transfer.take());
    if let Some(transfer) = active {
        if let Some(save_session_id) = transfer.save_session_id {
            astrobox_ng_wit::block_on(async {
                dialog::save_file_abort(save_session_id).await;
            });
        }
    }
    let should_continue = state::with_state(|state| {
        state.sync_failed += 1;
        state.last_status = format!("手表端文件同步失败：{}", reason);
        state.last_message = "screenshotSyncResult".to_string();
        !state.sync_queue.is_empty()
    });
    if should_continue {
        let _ = astrobox_ng_wit::block_on(async { start_next_interconnect_screenshot().await });
    }
    ui::rerender_if_possible();
    "screenshot-sync-result-failed".to_string()
}

fn handle_screenshot_fetch_progress_sync(message: ShellMessage) -> String {
    state::with_state(|state| {
        if let Some(transfer) = state.active_transfer.as_mut() {
            transfer.shot_id = if message.shot_id.is_empty() {
                "Fetch 批量同步".to_string()
            } else {
                message.shot_id.clone()
            };
            transfer.received = message.current;
            transfer.total = if state.sync_total > 0 {
                state.sync_total as i64
            } else {
                message.total
            };
            transfer.received_bytes = message.sent_bytes;
            transfer.size = message.total_bytes;
            transfer.rate_kbps = message.rate_kbps;
            state.last_status = format!(
                "Fetch 同步中：{}/{}，{:.1} KB/s",
                transfer.received, transfer.total, transfer.rate_kbps
            );
            state.last_message = "screenshotFetchProgress".to_string();
        }
    });
    ui::rerender_if_possible();
    "screenshot-fetch-progress-ok".to_string()
}

fn handle_screenshot_fetch_result_sync(message: ShellMessage) -> String {
    state::with_state(|state| {
        state.active_transfer = None;
        state.sync_done = message.done.max(0) as usize;
        state.sync_failed = message.failed.max(0) as usize;
        state.last_status = if message.success {
            format!(
                "Fetch 批量同步完成：成功 {}，失败 {}",
                state.sync_done, state.sync_failed
            )
        } else {
            let reason = if message.reason.is_empty() {
                "unknown"
            } else {
                message.reason.as_str()
            };
            format!(
                "Fetch 批量同步结束：成功 {}，失败 {}，原因 {}",
                state.sync_done, state.sync_failed, reason
            )
        };
        state.last_message = "screenshotFetchResult".to_string();
    });
    ui::rerender_if_possible();
    "screenshot-fetch-result-ok".to_string()
}

fn handle_screenshot_chunk_start_sync(message: ShellMessage) -> String {
    let session_id = message.session_id.clone();
    let ok = state::with_state(|state| {
        let Some(transfer) = state.active_transfer.as_mut() else {
            state.last_status = "收到截图开始包，但没有待保存任务".to_string();
            return false;
        };
        if !transfer.source_session_id.is_empty() && transfer.source_session_id != session_id {
            state.last_status = "收到其它截图会话开始包，已拒绝".to_string();
            return false;
        }
        if !message.shot_id.is_empty() && transfer.shot_id != message.shot_id {
            state.last_status = format!(
                "截图 ID 不匹配：等待 {}，收到 {}",
                transfer.shot_id, message.shot_id
            );
            return false;
        }
        transfer.source_session_id = session_id.clone();
        transfer.total = message.total;
        transfer.size = message.size;
        transfer.received = 0;
        state.last_status = format!(
            "开始接收文件：{}，{} 字节，共 {} 片",
            transfer.shot_id, transfer.size, transfer.total
        );
        state.last_message = "screenshotChunkStart".to_string();
        true
    });
    send_chunk_ack_sync(&session_id, "start", -1, ok);
    ui::rerender_if_possible();
    if ok {
        "screenshot-chunk-start-ok".to_string()
    } else {
        "screenshot-chunk-start-rejected".to_string()
    }
}

fn handle_screenshot_chunk_part_sync(message: ShellMessage) -> String {
    let session_id = message.session_id.clone();
    let index = message.index;
    let save_session_id = match state::with_state(|state| {
        let Some(transfer) = state.active_transfer.as_ref() else {
            state.last_status = "收到截图分片，但没有待保存任务".to_string();
            return None;
        };
        if transfer.source_session_id != session_id {
            state.last_status = "截图分片会话不匹配".to_string();
            return None;
        }
        if transfer.received != index {
            state.last_status = format!(
                "截图分片顺序不匹配：等待 {}，收到 {}",
                transfer.received, index
            );
            return None;
        }
        transfer.save_session_id
    }) {
        Some(save_session_id) => save_session_id,
        None => {
            send_chunk_ack_sync(&session_id, "part", index, false);
            ui::rerender_if_possible();
            return "screenshot-chunk-part-rejected".to_string();
        }
    };

    let bytes = match decode_base64(&message.d) {
        Ok(bytes) => bytes,
        Err(error) => {
            state::with_state(|state| {
                state.last_status = format!("截图分片解码失败：{}", error);
            });
            send_chunk_ack_sync(&session_id, "part", index, false);
            ui::rerender_if_possible();
            return "screenshot-chunk-part-decode-failed".to_string();
        }
    };

    let write_result = astrobox_ng_wit::block_on(async {
        dialog::save_file_write_chunk(save_session_id, &bytes).await
    });
    if write_result.is_err() {
        state::with_state(|state| {
            state.last_status = "写入截图分片失败".to_string();
        });
        send_chunk_ack_sync(&session_id, "part", index, false);
        ui::rerender_if_possible();
        return "screenshot-chunk-part-write-failed".to_string();
    }

    state::with_state(|state| {
        if let Some(transfer) = state.active_transfer.as_mut() {
            transfer.received += 1;
            transfer.received_bytes += bytes.len() as i64;
            let elapsed_ms = current_millis().saturating_sub(transfer.started_at_ms);
            transfer.rate_kbps = if elapsed_ms > 0 {
                (transfer.received_bytes as f64 / 1024.0) / (elapsed_ms as f64 / 1000.0)
            } else {
                0.0
            };
            state.last_status = format!(
                "正在保存文件：{}/{} 片，{:.1} KB/s",
                transfer.received, transfer.total, transfer.rate_kbps
            );
            state.last_message = "screenshotChunkPart".to_string();
        }
    });
    send_chunk_ack_sync(&session_id, "part", index, true);
    ui::rerender_if_possible();
    "screenshot-chunk-part-ok".to_string()
}

fn handle_screenshot_chunk_finish_sync(message: ShellMessage) -> String {
    let session_id = message.session_id.clone();
    let transfer = match state::with_state(|state| {
        let Some(transfer) = state.active_transfer.as_ref() else {
            state.last_status = "收到截图结束包，但没有待保存任务".to_string();
            return None;
        };
        if transfer.source_session_id != session_id {
            state.last_status = "截图结束会话不匹配".to_string();
            return None;
        }
        Some(transfer.clone())
    }) {
        Some(transfer) => transfer,
        None => {
            send_chunk_ack_sync(&session_id, "finish", -1, false);
            ui::rerender_if_possible();
            return "screenshot-chunk-finish-rejected".to_string();
        }
    };

    let finish_result = if let Some(save_session_id) = transfer.save_session_id {
        astrobox_ng_wit::block_on(async { dialog::save_file_finish(save_session_id).await })
    } else {
        Ok(())
    };
    if finish_result.is_err() {
        state::with_state(|state| {
            state.last_status = "完成截图保存失败".to_string();
        });
        send_chunk_ack_sync(&session_id, "finish", -1, false);
        ui::rerender_if_possible();
        return "screenshot-chunk-finish-failed".to_string();
    }

    let should_continue = state::with_state(|state| {
        state.active_transfer = None;
        state.sync_done += 1;
        state.last_status = format!(
            "文件已保存：{}（{}，平台 {}，{:.1} KB/s）",
            transfer.file_name, transfer.mode_label, transfer.platform, transfer.rate_kbps
        );
        state.last_message = "screenshotChunkFinish".to_string();
        !state.sync_queue.is_empty()
    });
    send_chunk_ack_sync(&session_id, "finish", -1, true);
    if should_continue {
        let _ = astrobox_ng_wit::block_on(async { start_next_interconnect_screenshot().await });
    }
    ui::rerender_if_possible();
    "screenshot-chunk-finish-ok".to_string()
}
