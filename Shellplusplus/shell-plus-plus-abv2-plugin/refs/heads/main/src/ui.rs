use astrobox_ng_wit::astrobox::psys_host::ui::{self, Element, ElementType, Event, FlexDirection};

use crate::state::{self, AppState};

pub const EVENT_REFRESH: &str = "shellpp_refresh_devices";
pub const EVENT_OPEN_APP: &str = "shellpp_open_app";
pub const EVENT_HANDSHAKE: &str = "shellpp_handshake";
pub const EVENT_PANEL_DEVICE: &str = "shellpp_panel_device";
pub const EVENT_PANEL_SCREENSHOT: &str = "shellpp_panel_screenshot";
pub const EVENT_PANEL_LOGS: &str = "shellpp_panel_logs";
pub const EVENT_PANEL_TERMINAL: &str = "shellpp_panel_terminal";
pub const EVENT_PANEL_DEBUG: &str = "shellpp_panel_debug";
pub const EVENT_REQUEST_LIST: &str = "shellpp_request_screenshot_list";
pub const EVENT_REQUEST_RAW_LIST: &str = "shellpp_request_raw_list";
pub const EVENT_SYNC_SELECT: &str = "shellpp_sync_select";
pub const EVENT_TOGGLE_SELECT_ALL: &str = "shellpp_toggle_select_all";
pub const EVENT_START_SELECTED_SYNC: &str = "shellpp_start_selected_sync";
pub const EVENT_CANCEL_SELECTION: &str = "shellpp_cancel_selection";
pub const EVENT_TOGGLE_TRANSFER_MODE: &str = "shellpp_toggle_transfer_mode";
pub const EVENT_SET_FETCH_URL: &str = "shellpp_set_fetch_url";
pub const EVENT_EXEC_COMMAND: &str = "shellpp_exec_command";
pub const EVENT_TERMINAL_INPUT: &str = "shellpp_terminal_input";
pub const EVENT_EXEC_TERMINAL_INPUT: &str = "shellpp_exec_terminal_input";
pub const EVENT_SYNC_RAW: &str = "shellpp_sync_raw";
pub const EVENT_TOGGLE_SCREENSHOT_PREFIX: &str = "shellpp_toggle_screenshot_";
pub const EVENT_CLEAR: &str = "shellpp_clear";
pub const EVENT_TOGGLE_CLI: &str = "shellpp_toggle_cli";

pub fn is_known_event(event_id: &str) -> bool {
    matches!(
        event_id,
        EVENT_REFRESH
            | EVENT_OPEN_APP
            | EVENT_HANDSHAKE
            | EVENT_PANEL_DEVICE
            | EVENT_PANEL_SCREENSHOT
            | EVENT_PANEL_LOGS
            | EVENT_PANEL_TERMINAL
            | EVENT_PANEL_DEBUG
            | EVENT_REQUEST_LIST
            | EVENT_REQUEST_RAW_LIST
            | EVENT_SYNC_SELECT
            | EVENT_TOGGLE_SELECT_ALL
            | EVENT_START_SELECTED_SYNC
            | EVENT_CANCEL_SELECTION
            | EVENT_TOGGLE_TRANSFER_MODE
            | EVENT_SET_FETCH_URL
            | EVENT_EXEC_COMMAND
            | EVENT_EXEC_TERMINAL_INPUT
            | EVENT_SYNC_RAW
            | EVENT_CLEAR
            | EVENT_TOGGLE_CLI
    ) || event_id.starts_with(EVENT_TOGGLE_SCREENSHOT_PREFIX)
}

pub fn is_text_input_event(event_id: &str) -> bool {
    event_id == EVENT_TERMINAL_INPUT
}

pub fn is_dialog_like_event(event_id: &str) -> bool {
    event_id == EVENT_EXEC_COMMAND || event_id == EVENT_EXEC_TERMINAL_INPUT
}

pub fn is_navigation_event(event_id: &str) -> bool {
    matches!(
        event_id,
        EVENT_PANEL_DEVICE
            | EVENT_PANEL_SCREENSHOT
            | EVENT_PANEL_LOGS
            | EVENT_PANEL_TERMINAL
            | EVENT_PANEL_DEBUG
    )
}

pub fn render_main_ui(element_id: &str) {
    ui::render(element_id, build_main_ui(&state::snapshot()));
}

pub fn rerender_if_possible() {
    if let Some(root_element_id) = state::snapshot().root_element_id {
        render_main_ui(&root_element_id);
    }
}

fn build_main_ui(state: &AppState) -> Element {
    let connection_text = connection_text(state);
    let status_color = if state.connected {
        "#25C281"
    } else {
        "#FFB454"
    };

    Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(12)
        .bg("#07090D")
        .child(text("Shell++", 24, "#FFFFFF"))
        .child(text("AstroBoxV2 ", 13, "#9AA6B8"))
        .child(status_badge(&connection_text, status_color))
        .child(navigation(state))
        .child(match state.active_panel.as_str() {
            "screenshot" => screenshot_panel(state),
            "terminal" => terminal_panel(state),
            "debug" => debug_panel(state),
            "logs" => logs_panel(state),
            _ => device_panel(state),
        })
}

fn connection_text(state: &AppState) -> String {
    if state.connected {
        "已连接 Shell++".to_string()
    } else if state.registered_recv {
        "已注册接收，等待握手".to_string()
    } else {
        "尚未连接".to_string()
    }
}

fn navigation(state: &AppState) -> Element {
    Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Row)
        .width_full()
        .padding(4)
        .margin_top(8)
        .margin_bottom(8)
        .radius(999)
        .bg("#121923")
        .border(1, "#2B394A")
        .child(tab_button(
            "连接",
            EVENT_PANEL_DEVICE,
            state.active_panel == "device",
        ))
        .child(tab_button(
            "截图",
            EVENT_PANEL_SCREENSHOT,
            state.active_panel == "screenshot",
        ))
        .child(tab_button(
            "终端",
            EVENT_PANEL_TERMINAL,
            state.active_panel == "terminal",
        ))
        // .child(tab_button(
        //     "Debug",
        //     EVENT_PANEL_DEBUG,
        //     state.active_panel == "debug",
        // ))
        .child(tab_button(
            "日志",
            EVENT_PANEL_LOGS,
            state.active_panel == "logs",
        ))
}

fn device_panel(state: &AppState) -> Element {
    let device_text = match &state.selected_device {
        Some(device) => format!("{}\n{}", device.name, device.addr),
        None => "暂无已连接设备，点击刷新".to_string(),
    };
    // let platform_text = if state.host_platform.is_empty() {
    //     "待检测".to_string()
    // } else {
    //     state.host_platform.clone()
    // };

    let mut actions = action_card("连接操作")
        .child(button("刷新设备", EVENT_REFRESH, "#252525"))
        .child(button("打开 Shell++", EVENT_OPEN_APP, "#0D6EFF"))
        .child(button("握手连接", EVENT_HANDSHAKE, "#0D6EFF"));

    if state.selected_device.is_none() {
        actions = actions.child(text("请先在 AstroBox 中连接手表", 13, "#8C94A3"));
    }

    panel_shell("连接与设备", "管理手表、快应用与宿主连接")
        .child(info_card("当前设备", &device_text))
        .child(info_card(
            "连接状态",
            &format!("{}\n{}", connection_text(state), state.last_status),
        ))
        // .child(info_card(
        //     "运行环境",
        //     &format!("宿主：{}\n目标：{}", platform_text, state.target_pkg_name),
        // ))
        .child(actions)
}

fn screenshot_panel(state: &AppState) -> Element {
    let transfer_text = transfer_status_text(state);

    let selected_count = state.selected_shot_ids.len();
    let mode_text = if state.sync_mode == "fetch" {
        let url = if state.fetch_url.is_empty() {
            "未设置 URL"
        } else {
            state.fetch_url.as_str()
        };
        format!("Fetch 直传\n{}\n已选 {} 张", url, selected_count)
    } else {
        format!("Interconnect 保存\n已选 {} 张", selected_count)
    };

    let mut actions = action_card("同步操作")
        .child(button("拉取截图列表", EVENT_REQUEST_LIST, "#252525"))
        .child(button(
            "切换传输模式",
            EVENT_TOGGLE_TRANSFER_MODE,
            "#252525",
        ));

    if state.sync_mode == "fetch" {
        actions = actions.child(button("设置 Fetch URL", EVENT_SET_FETCH_URL, "#252525"));
    }

    if state.selecting_screenshots {
        actions = actions
            .child(button(
                "全选 / 取消全选",
                EVENT_TOGGLE_SELECT_ALL,
                "#252525",
            ))
            .child(button(
                "开始同步选中截图",
                EVENT_START_SELECTED_SYNC,
                "#0D6EFF",
            ))
            .child(button("退出选择", EVENT_CANCEL_SELECTION, "#252525"));
    } else {
        actions = actions.child(button("选择并同步截图", EVENT_SYNC_SELECT, "#0D6EFF"));
    }

    panel_shell("截图同步", "拉取、选择并保存手表截图")
        .child(info_card("传输状态", &transfer_text))
        .child(info_card("传输方式", &mode_text))
        .child(actions)
        .child(screenshot_list(state))
}

fn screenshot_list(state: &AppState) -> Element {
    let title = format!("截图列表 · {} 张", state.screenshots.len());
    let mut list = Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(12)
        .margin_top(8)
        .radius(14)
        .bg("#141C27")
        .border(1, "#344355")
        .opacity(0.92)
        .child(text(&title, 18, "#FFFFFF"));

    if state.screenshots.is_empty() {
        return list.child(text("暂无截图，先拉取列表", 14, "#8C94A3"));
    }

    for (index, item) in state.screenshots.iter().take(12).enumerate() {
        let selected = state.selected_shot_ids.iter().any(|id| id == &item.shot_id);
        let prefix = if state.selecting_screenshots {
            if selected { "[已选] " } else { "[ ] " }
        } else {
            ""
        };
        let name = if item.shot_id.is_empty() {
            "未命名截图".to_string()
        } else {
            format!("{}{}", prefix, item.shot_id)
        };
        let captured_at = if item.captured_at.is_empty() {
            format!("序号 {}", item.index)
        } else {
            item.captured_at.clone()
        };
        let mut row = Element::new(ElementType::Div, None)
            .flex()
            .flex_direction(FlexDirection::Column)
            .width_full()
            .padding(10)
            .margin_top(8)
            .radius(12)
            .bg(if selected { "#172A4D" } else { "#1B1B1B" })
            .child(text(&name, 16, "#FFFFFF"))
            .child(text(&captured_at, 13, "#8FB5FF"));

        if state.selecting_screenshots {
            row = row.child(button(
                if selected { "取消选择" } else { "选择" },
                &format!("{}{}", EVENT_TOGGLE_SCREENSHOT_PREFIX, index),
                if selected { "#263342" } else { "#2B67C7" },
            ));
        }

        list = list.child(row);
    }

    list
}

fn transfer_status_text(state: &AppState) -> String {
    if let Some(transfer) = state.active_transfer.as_ref() {
        let progress = if transfer.total > 0 {
            format!("{}/{} 片", transfer.received, transfer.total)
        } else {
            "准备中".to_string()
        };
        return format!(
            "{}\n{} · {} · {:.1} KB/s\n已接收 {} / {} 字节",
            transfer.shot_id,
            transfer.mode_label,
            progress,
            transfer.rate_kbps,
            transfer.received_bytes,
            transfer.size
        );
    }

    if state.sync_total > 0 {
        return format!(
            "{}\n完成 {}/{} · 失败 {}",
            state.last_status, state.sync_done, state.sync_total, state.sync_failed
        );
    }

    format!("{}\n暂无传输任务", state.last_status)
}

fn terminal_panel(state: &AppState) -> Element {
    let last_command = if state.terminal_last_command.is_empty() {
        "暂无命令".to_string()
    } else {
        state.terminal_last_command.clone()
    };
    panel_shell("终端", "粘贴命令后直接执行，不再弹二次输入框")
        .child(info_card(
            "命令状态",
            &format!("{}\n{}", state.terminal_status, last_command),
        ))
        .child(command_input_card(state))
        .child(copyable_output_card(
            "终端输出（逐行显示）",
            &state.terminal_output,
        ))
}

fn command_input_card(state: &AppState) -> Element {
    action_card("命令输入")
        .child(text("在下面输入或粘贴命令，然后点击执行", 13, "#8C94A3"))
        .child(
            Element::new(ElementType::Input, Some(state.terminal_input.as_str()))
                .width_full()
                .height(56)
                .margin_top(8)
                .padding(10)
                .radius(12)
                .bg("#0F1722")
                .border(1, "#344355")
                .text_color("#FFFFFF")
                .on(Event::Change, EVENT_TERMINAL_INPUT)
                .on(Event::Input, EVENT_TERMINAL_INPUT),
        )
        .child(button("执行输入命令", EVENT_EXEC_TERMINAL_INPUT, "#0D6EFF"))
}

fn copyable_output_card(title: &str, body: &str) -> Element {
    let mut output = Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(10)
        .margin_top(6)
        .radius(12)
        .bg("#0B111A")
        .border(1, "#2D3A4B");

    for line in terminal_output_lines(body) {
        output = output.child(
            Element::new(ElementType::P, Some(line.as_str()))
                .width_full()
                .margin_bottom(2)
                .text_color("#D8DCE3")
                .size(13),
        );
    }

    Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(10)
        .margin_top(8)
        .radius(14)
        .bg("#141C27")
        .border(1, "#344355")
        .opacity(0.92)
        .child(text(title, 14, "#8FB5FF"))
        .child(output)
        .child(text("复制用（完整输出）", 12, "#8C94A3"))
        .child(
            Element::new(ElementType::Input, Some(body))
                .width_full()
                .height(42)
                .margin_top(4)
                .padding(8)
                .radius(10)
                .bg("#0F1722")
                .border(1, "#344355")
                .text_color("#D8DCE3")
                .size(12),
        )
}

fn terminal_output_lines(body: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    for raw_line in normalized.split('\n') {
        if raw_line.is_empty() {
            rows.push(" ".to_string());
            continue;
        }
        let chars = raw_line.chars().collect::<Vec<_>>();
        if chars.len() <= 72 {
            rows.push(raw_line.to_string());
            continue;
        }
        let mut start = 0usize;
        while start < chars.len() {
            let end = (start + 72).min(chars.len());
            rows.push(chars[start..end].iter().collect::<String>());
            start = end;
        }
    }
    if rows.is_empty() {
        rows.push("暂无输出".to_string());
    }
    if rows.len() > 260 {
        rows.truncate(260);
        rows.push("...输出过长，完整内容请使用下方复制框".to_string());
    }
    rows
}

fn debug_panel(state: &AppState) -> Element {
    let raw_items = state
        .screenshots
        .iter()
        .filter(|item| is_raw_screenshot(item))
        .collect::<Vec<_>>();
    let latest = raw_items
        .first()
        .map(|item| format!("{}\n{}", item.shot_id, item.captured_at))
        .unwrap_or_else(|| "暂无 .raw 文件，先拉取 RAW 列表".to_string());
    let mut raw_list = action_card(&format!("RAW 文件 · {} 个", raw_items.len()))
        .child(button("拉取 RAW 列表", EVENT_REQUEST_RAW_LIST, "#252525"))
        .child(button("同步最新 .raw 文件", EVENT_SYNC_RAW, "#0D6EFF"));
    for item in raw_items.iter().take(8) {
        raw_list = raw_list.child(info_card(
            &item.shot_id,
            if item.captured_at.is_empty() {
                "RAW 调试文件"
            } else {
                item.captured_at.as_str()
            },
        ));
    }
    let cli_status = if !state.cli_registered {
        "注册失败：当前插件实例未获得 Deeplink 事件订阅"
    } else if state.cli_enabled {
        "已注册并开启：允许 astrobox-cli 通过 Deeplink 调用插件功能"
    } else {
        "已注册但关闭：所有 CLI Deeplink 请求都会被拒绝"
    };
    let cli_button = if state.cli_enabled {
        "关闭 CLI 调用"
    } else {
        "开启 CLI 调用"
    };
    let cli_color = if state.cli_enabled {
        "#C24141"
    } else {
        "#0D6EFF"
    };
    let cli_card = action_card("CLI 调用开关")
        .child(info_card("当前状态", cli_status))
        .child(button(cli_button, EVENT_TOGGLE_CLI, cli_color));

    panel_shell("Debug", "调试文件同步与协议排查")
        .child(cli_card)
        .child(info_card("传输状态", &transfer_status_text(state)))
        .child(info_card("最新 RAW", &latest))
        .child(raw_list)
        .child(info_card(
            "说明",
            "接收逻辑沿用截图分片协议，仅保存扩展名为 .raw 的源文件",
        ))
}

fn is_raw_screenshot(item: &crate::protocol::ScreenshotItem) -> bool {
    let value = item.shot_id.to_ascii_lowercase();
    let source = item.source.to_ascii_lowercase();
    source == "framebuffer_raw"
        || value.ends_with(".raw")
        || value.contains(".raw#")
        || value.contains("_raw")
}

fn logs_panel(state: &AppState) -> Element {
    let message = if state.last_message.is_empty() {
        state.last_status.clone()
    } else {
        format!("{}\n最后消息：{}", state.last_status, state.last_message)
    };

    panel_shell("运行日志", "查看连接、传输和错误信息")
        .child(info_card("当前状态", &message))
        .child(log_list(state))
        .child(button("清空状态与日志", EVENT_CLEAR, "#252525"))
}

fn log_list(state: &AppState) -> Element {
    let mut logs = Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(12)
        .margin_top(8)
        .radius(16)
        .bg("#141C27")
        .border(1, "#344355")
        .opacity(0.92)
        .child(text("最近日志", 16, "#FFFFFF"));

    if state.logs.is_empty() {
        logs = logs.child(text("暂无日志", 14, "#8C94A3"));
    } else {
        for line in state.logs.iter().rev().take(12).rev() {
            logs = logs.child(
                Element::new(ElementType::Div, None)
                    .width_full()
                    .padding(6)
                    .margin_top(6)
                    .radius(9)
                    .bg("#1B2430")
                    .border(1, "#2D3A4B")
                    .child(text(line, 12, "#B8BEC9")),
            );
        }
    }

    logs
}

fn panel_shell(title: &str, description: &str) -> Element {
    Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .child(text(title, 20, "#FFFFFF"))
        .child(text(description, 12, "#9AA6B8"))
}

fn info_card(title: &str, body: &str) -> Element {
    Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(10)
        .margin_top(8)
        .radius(14)
        .bg("#141C27")
        .border(1, "#344355")
        .opacity(0.92)
        .child(text(title, 14, "#8FB5FF"))
        .child(text(body, 13, "#D8DCE3"))
}

fn action_card(title: &str) -> Element {
    Element::new(ElementType::Div, None)
        .flex()
        .flex_direction(FlexDirection::Column)
        .width_full()
        .padding(10)
        .margin_top(8)
        .radius(14)
        .bg("#141C27")
        .border(1, "#344355")
        .opacity(0.92)
        .child(text(title, 15, "#FFFFFF"))
}

fn status_badge(label: &str, color: &str) -> Element {
    Element::new(ElementType::P, Some(label))
        .margin_top(8)
        .padding(6)
        .radius(999)
        .bg("#182536")
        .border(1, "#3D536E")
        .text_color(color)
        .size(13)
}

fn tab_button(label: &str, event_id: &str, active: bool) -> Element {
    Element::new(ElementType::Button, Some(label))
        .padding(7)
        .margin_right(4)
        .radius(999)
        .bg(if active { "#2B67C7" } else { "#1D2733" })
        .border(1, if active { "#6B9BEA" } else { "#344355" })
        .text_color("#FFFFFF")
        .on(Event::Click, event_id)
        .on(Event::PointerUp, event_id)
}

fn button(label: &str, event_id: &str, color: &str) -> Element {
    Element::new(ElementType::Button, Some(label))
        .width_full()
        .margin_top(6)
        .padding(7)
        .radius(11)
        .bg(if color == "#252525" {
            "#1D2733"
        } else {
            "#2B67C7"
        })
        .border(
            1,
            if color == "#252525" {
                "#344355"
            } else {
                "#6B9BEA"
            },
        )
        .text_color("#FFFFFF")
        .on(Event::Click, event_id)
        .on(Event::PointerUp, event_id)
}

fn text(content: &str, size: u32, color: &str) -> Element {
    Element::new(ElementType::P, Some(content))
        .size(size)
        .text_color(color)
        .margin_bottom(2)
}
