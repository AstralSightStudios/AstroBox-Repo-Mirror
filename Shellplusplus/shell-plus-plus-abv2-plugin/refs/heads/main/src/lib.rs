use astrobox_ng_wit::FutureReader;
use astrobox_ng_wit::astrobox::psys_host::{self, register};
use astrobox_ng_wit::exports::astrobox::psys_plugin::{
    event::{self, EventType},
    lifecycle,
};

pub mod logger;
pub mod protocol;
pub mod shell_sync;
pub mod state;
pub mod ui;

struct ShellPlusPlusPlugin;

impl event::Guest for ShellPlusPlusPlugin {
    fn on_event(event_type: EventType, event_payload: String) -> FutureReader<String> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(|| "".to_string());

        match event_type {
            EventType::InterconnectMessage => {
                let result = shell_sync::handle_interconnect_message_sync(&event_payload);
                astrobox_ng_wit::spawn(async move {
                    let _ = writer.write(result).await;
                });
            }
            EventType::DeeplinkAction => {
                state::append_log(format!(
                    "[CLI] DeeplinkAction 已到达，payload_bytes={}",
                    event_payload.len()
                ));
                ui::rerender_if_possible();
                astrobox_ng_wit::spawn(async move {
                    let result = handle_deeplink_action(&event_payload).await;
                    let _ = writer.write(result).await;
                });
            }
            _ => {
                astrobox_ng_wit::spawn(async move {
                    let _ = writer.write("ignored".to_string()).await;
                });
            }
        }

        reader
    }

    fn on_ui_event(
        event_id: String,
        event: event::Event,
        event_payload: String,
    ) -> FutureReader<String> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(|| "".to_string());
        let is_text_input = matches!(event, event::Event::Change | event::Event::Input)
            && ui::is_text_input_event(&event_id);
        if is_text_input {
            handle_text_input_event(&event_id, &event_payload);
            astrobox_ng_wit::spawn(async move {
                let _ = writer.write("accepted".to_string()).await;
            });
            return reader;
        }

        let is_action = matches!(event, event::Event::Click | event::Event::PointerUp)
            && ui::is_known_event(&event_id);

        if !is_action {
            astrobox_ng_wit::spawn(async move {
                let _ = writer.write("unknown-ui-event".to_string()).await;
            });
            return reader;
        }

        if is_duplicate_ui_event(&event_id) {
            astrobox_ng_wit::spawn(async move {
                let _ = writer.write("accepted".to_string()).await;
            });
            return reader;
        }

        let is_navigation = ui::is_navigation_event(&event_id);
        if !is_navigation {
            state::with_state(|state| {
                state.last_status = action_pending_message(&event_id).to_string();
            });
            ui::rerender_if_possible();
        }

        let result = handle_ui_action_sync(&event_id);
        state::with_state(|state| {
            if result != "refresh-ok"
                && result != "launch-sent"
                && result != "handshake-sent"
                && result != "request-list-sent"
                && result != "raw-list-request-sent"
                && result != "exec-command-sent"
                && result != "raw-sync-sent"
                && result != "sync-request-sent"
                && result != "selection-updated"
                && result != "mode-updated"
                && result != "panel-updated"
                && result != "cleared"
            {
                state.last_status = result;
            }
        });
        ui::rerender_if_possible();

        astrobox_ng_wit::spawn(async move {
            let _ = writer.write("accepted".to_string()).await;
        });

        reader
    }

    fn on_ui_render(element_id: String) -> FutureReader<()> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<()>(|| ());

        state::with_state(|state| {
            state.root_element_id = Some(element_id.clone());
        });
        ui::render_main_ui(&element_id);

        astrobox_ng_wit::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }

    fn on_card_render(_card_id: String) -> FutureReader<()> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<()>(|| ());

        astrobox_ng_wit::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }
}

impl lifecycle::Guest for ShellPlusPlusPlugin {
    fn on_load() {
        logger::init();
        tracing::info!("Shell++ AstroBoxV2 plugin loaded");
        match astrobox_ng_wit::block_on(async {
            psys_host::register::register_deeplink_action().await
        }) {
            Ok(()) => {
                state::with_state(|state| state.cli_registered = true);
                state::append_log("[CLI] Deeplink 入口已在 on_load 生命周期内注册");
            }
            Err(error) => {
                state::with_state(|state| state.cli_registered = false);
                state::append_warn(format!(
                    "[CLI] Deeplink 入口同步注册失败，请检查权限: {:?}",
                    error
                ));
            }
        }
        astrobox_ng_wit::spawn(async move {
            let _ = psys_host::register::register_card(
                register::CardType::Element,
                "shellpp-status-card",
                "Shell++ 状态",
            )
            .await;
            if let Err(error) = shell_sync::bootstrap().await {
                tracing::warn!("Shell++ bootstrap failed: {}", error);
                state::with_state(|state| {
                    state.last_status = format!("等待设备连接：{}", error);
                });
            }
            ui::rerender_if_possible();
        });
    }
}

async fn handle_deeplink_action(event_payload: &str) -> String {
    if !state::snapshot().cli_enabled {
        let message = "CLI 调用已在 Debug 面板关闭";
        state::append_warn(format!("[CLI] 已拒绝：{}", message));
        ui::rerender_if_possible();
        return deeplink_response(false, "disabled", message);
    }
    state::append_log("[CLI] 开关已开启，开始解析请求");
    let request = match protocol::parse_deeplink_action_request(event_payload) {
        Ok(request) => request,
        Err(message) => {
            state::append_warn(format!("[CLI] 请求解析失败：{}", message));
            ui::rerender_if_possible();
            return deeplink_response(false, "invalid", &message);
        }
    };

    state::append_log(format!(
        "[CLI] 请求已解析：action={}, cmd_bytes={}, callback={}",
        request.action,
        request.cmd.len(),
        if request.callback.is_empty() {
            "none"
        } else {
            "loopback"
        }
    ));
    let (ok, result) = match request.action.as_str() {
        "status" => {
            let snapshot = state::snapshot();
            let result = serde_json::json!({
                "connected": snapshot.connected,
                "registeredRecv": snapshot.registered_recv,
                "device": snapshot.selected_device.map(|device| serde_json::json!({
                    "name": device.name,
                    "addr": device.addr
                })),
                "package": snapshot.target_pkg_name,
                "lastStatus": snapshot.last_status
            })
            .to_string();
            (true, result)
        }
        "refresh" | "refresh-devices" => {
            let result = shell_sync::refresh_devices().await;
            (result == "refresh-ok", result)
        }
        "launch" | "launch-app" => {
            let result = shell_sync::launch_quick_app().await;
            (result == "launch-sent", result)
        }
        "handshake" => {
            let result = shell_sync::handshake().await;
            (result == "handshake-sent", result)
        }
        "screenshots" | "request-screenshot-list" => {
            let result = shell_sync::request_screenshot_list().await;
            (result == "request-list-sent", result)
        }
        "raws" | "request-raw-list" => {
            let result = shell_sync::request_raw_list().await;
            (result == "raw-list-request-sent", result)
        }
        "exec" | "exec-command" => {
            let result = shell_sync::exec_command_with_callback(&request.cmd, &request.callback).await;
            (result == "exec-command-sent", result)
        }
        "sync-latest-raw" => {
            let result = shell_sync::sync_latest_raw().await;
            let ok = result == "raw-sync-sent" || result.starts_with("暂无 RAW 文件");
            (ok, result)
        }
        "panel" | "set-panel" => match request.panel.as_str() {
            "device" | "screenshot" | "terminal" | "debug" | "logs" => {
                (true, set_active_panel(&request.panel))
            }
            _ => (
                false,
                "panel 可用值: device, screenshot, terminal, debug, logs".to_string(),
            ),
        },
        "clear" | "clear-state" => (true, clear_plugin_state()),
        _ => (
            false,
            "不支持的 action；可用值: status, refresh-devices, launch-app, handshake, request-screenshot-list, request-raw-list, exec, sync-latest-raw, set-panel, clear-state"
                .to_string(),
        ),
    };

    state::append_log(format!(
        "[CLI] action={} 初始结果：{}",
        request.action, result
    ));
    state::with_state(|state| {
        state.last_status = if ok {
            format!("CLI {} 执行完成", request.action)
        } else {
            format!("CLI {} 执行失败: {}", request.action, result)
        };
    });
    ui::rerender_if_possible();
    deeplink_response(ok, &request.action, &result)
}

fn deeplink_response(ok: bool, action: &str, result: &str) -> String {
    serde_json::json!({
        "ok": ok,
        "action": action,
        "result": result
    })
    .to_string()
}

fn handle_text_input_event(event_id: &str, event_payload: &str) {
    let value = parse_input_value(event_payload);
    if event_id == ui::EVENT_TERMINAL_INPUT {
        state::with_state(|state| {
            state.terminal_input = value;
            state.active_panel = "terminal".to_string();
            state.terminal_status = "命令已输入，点击执行即可发送".to_string();
        });
    }
}

fn parse_input_value(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        json.get("value")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        payload.to_string()
    }
}

fn action_pending_message(event_id: &str) -> &'static str {
    match event_id {
        ui::EVENT_REFRESH => "正在刷新设备并申请 device 权限...",
        ui::EVENT_OPEN_APP => "正在打开 Shell++ 快应用...",
        ui::EVENT_HANDSHAKE => "正在申请通信权限并握手...",
        ui::EVENT_REQUEST_LIST => "正在申请通信权限并拉取截图列表...",
        ui::EVENT_REQUEST_RAW_LIST => "正在申请通信权限并拉取 RAW 列表...",
        ui::EVENT_SYNC_SELECT => "正在进入截图选择模式...",
        ui::EVENT_TOGGLE_SELECT_ALL => "正在切换全选状态...",
        ui::EVENT_START_SELECTED_SYNC => "正在开始批量同步...",
        ui::EVENT_CANCEL_SELECTION => "正在取消选择...",
        ui::EVENT_TOGGLE_TRANSFER_MODE => "正在切换传输模式...",
        ui::EVENT_SET_FETCH_URL => "正在设置 Fetch URL...",
        ui::EVENT_EXEC_COMMAND => "正在打开命令输入框...",
        ui::EVENT_EXEC_TERMINAL_INPUT => "正在执行输入框命令...",
        ui::EVENT_SYNC_RAW => "正在准备同步 RAW 文件...",
        ui::EVENT_CLEAR => "正在清空状态...",
        ui::EVENT_TOGGLE_CLI => "正在切换 CLI 调用开关...",
        _ if event_id.starts_with(ui::EVENT_TOGGLE_SCREENSHOT_PREFIX) => "正在切换截图选择...",
        _ => "正在处理...",
    }
}

fn current_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn is_duplicate_ui_event(event_id: &str) -> bool {
    let now = current_millis();
    state::with_state(|state| {
        let duplicated = state.last_ui_event_id == event_id
            && now.saturating_sub(state.last_ui_event_at_ms)
                < if ui::is_dialog_like_event(event_id) {
                    1800
                } else {
                    350
                };
        if !duplicated {
            state.last_ui_event_id = event_id.to_string();
            state.last_ui_event_at_ms = now;
        }
        duplicated
    })
}

fn handle_ui_action_sync(event_id: &str) -> String {
    match event_id {
        ui::EVENT_REFRESH => {
            astrobox_ng_wit::block_on(async { shell_sync::refresh_devices().await })
        }
        ui::EVENT_OPEN_APP => {
            astrobox_ng_wit::block_on(async { shell_sync::launch_quick_app().await })
        }
        ui::EVENT_HANDSHAKE => astrobox_ng_wit::block_on(async { shell_sync::handshake().await }),
        ui::EVENT_PANEL_DEVICE => set_active_panel("device"),
        ui::EVENT_PANEL_SCREENSHOT => set_active_panel("screenshot"),
        ui::EVENT_PANEL_TERMINAL => set_active_panel("terminal"),
        ui::EVENT_PANEL_DEBUG => set_active_panel("debug"),
        ui::EVENT_PANEL_LOGS => set_active_panel("logs"),
        ui::EVENT_REQUEST_LIST => {
            astrobox_ng_wit::block_on(async { shell_sync::request_screenshot_list().await })
        }
        ui::EVENT_REQUEST_RAW_LIST => {
            astrobox_ng_wit::block_on(async { shell_sync::request_raw_list().await })
        }
        ui::EVENT_SYNC_SELECT => shell_sync::enter_screenshot_selection(),
        ui::EVENT_TOGGLE_SELECT_ALL => shell_sync::toggle_select_all_screenshots(),
        ui::EVENT_START_SELECTED_SYNC => {
            astrobox_ng_wit::block_on(async { shell_sync::start_selected_screenshots().await })
        }
        ui::EVENT_CANCEL_SELECTION => shell_sync::cancel_screenshot_selection(),
        ui::EVENT_TOGGLE_TRANSFER_MODE => shell_sync::toggle_transfer_mode(),
        ui::EVENT_SET_FETCH_URL => {
            astrobox_ng_wit::block_on(async { shell_sync::set_fetch_url().await })
        }
        ui::EVENT_EXEC_COMMAND => {
            astrobox_ng_wit::block_on(async { shell_sync::prompt_and_exec_command().await })
        }
        ui::EVENT_EXEC_TERMINAL_INPUT => {
            astrobox_ng_wit::block_on(async { shell_sync::exec_terminal_input().await })
        }
        ui::EVENT_SYNC_RAW => {
            astrobox_ng_wit::block_on(async { shell_sync::sync_latest_raw().await })
        }
        ui::EVENT_CLEAR => clear_plugin_state(),
        ui::EVENT_TOGGLE_CLI => toggle_cli_enabled(),
        _ if event_id.starts_with(ui::EVENT_TOGGLE_SCREENSHOT_PREFIX) => {
            let index_text = event_id.trim_start_matches(ui::EVENT_TOGGLE_SCREENSHOT_PREFIX);
            match index_text.parse::<usize>() {
                Ok(index) => shell_sync::toggle_screenshot_selection(index),
                Err(_) => "截图索引无效".to_string(),
            }
        }
        _ => "ignored".to_string(),
    }
}

fn set_active_panel(panel: &str) -> String {
    state::with_state(|state| {
        state.active_panel = panel.to_string();
    });
    "panel-updated".to_string()
}

fn toggle_cli_enabled() -> String {
    let enabled = state::with_state(|state| {
        state.cli_enabled = !state.cli_enabled;
        state.last_status = if state.cli_enabled {
            "CLI 调用已开启".to_string()
        } else {
            "CLI 调用已关闭".to_string()
        };
        state.cli_enabled
    });
    if enabled {
        "cli-enabled".to_string()
    } else {
        "cli-disabled".to_string()
    }
}

fn clear_plugin_state() -> String {
    state::with_state(|state| {
        state.screenshots.clear();
        state.active_transfer = None;
        state.selected_shot_ids.clear();
        state.selecting_screenshots = false;
        state.sync_queue.clear();
        state.sync_total = 0;
        state.sync_done = 0;
        state.sync_failed = 0;
        state.last_status = "状态已清空".to_string();
        state.last_message.clear();
        state.terminal_status = "等待输入命令".to_string();
        state.terminal_input.clear();
        state.terminal_output = "暂无输出".to_string();
        state.terminal_last_command.clear();
        state.pending_exec_req_id.clear();
        state.logs.clear();
    });
    "cleared".to_string()
}

astrobox_ng_wit::export!(ShellPlusPlusPlugin);
