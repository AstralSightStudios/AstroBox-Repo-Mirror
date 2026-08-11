//! 插件主界面

use astrobox_ng_wit::astrobox::psys_host::{self, dialog, ui_v3};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

use crate::parser;

pub const EVENT_PICK_FILE: &str = "pick_file";
pub const EVENT_SEND_TO_WATCH: &str = "send_to_watch";
pub const EVENT_MOUSE_LEAVE: &str = "button_mouse_leave";
pub const EVENT_COPY_LOG: &str = "copy_log";
pub const PKG_NAME: &str = "com.schedule.vela";
pub const ENTRY_PAGE: &str = "pages/index";

const MAX_LOG: usize = 30;
const RADIUS: u32 = 12;
const LOG_HEIGHT: u32 = 400;
const LOG_SCROLL_MAX: u32 = 100_000;
const CARD_BG: &str = "#1E1E1F";
const BTN_BG: &str = "#2A2A2A";
const BTN_HOVER: &str = "#4b4b4b";
const TEXT: &str = "#E6E6E6";
const MUTED: &str = "#8a8a8a";
const LOG_C: &str = "#bbbbbb";

struct UiState {
    root: Option<String>,
    device_addr: String,
    file_name: String,
    parsed: Option<Value>,
    log: Vec<String>,
    hovered: Option<String>,
    pending_scroll: bool,
}

static STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

fn state() -> &'static Mutex<UiState> {
    STATE.get_or_init(|| {
        Mutex::new(UiState {
            root: None,
            device_addr: String::new(),
            file_name: String::new(),
            parsed: None,
            log: Vec::new(),
            hovered: None,
            pending_scroll: false,
        })
    })
}

fn p(text: &str) -> ui_v3::Element {
    ui_v3::Element::new(ui_v3::ElementType::P, Some(text))
}

fn btn(label: &str, id: &str, hovered: bool) -> ui_v3::Element {
    ui_v3::Element::new(ui_v3::ElementType::Button, Some(label))
        .without_default_styles()
        .on(ui_v3::Event::Click, id)
        .on(ui_v3::Event::MouseEnter, id)
        .on(ui_v3::Event::MouseLeave, EVENT_MOUSE_LEAVE)
        .radius(RADIUS)
        .padding(12)
        .flex_grow(1.0)
        .bg(if hovered { BTN_HOVER } else { BTN_BG })
        .text_color(TEXT)
}

fn col() -> ui_v3::Element {
    ui_v3::Element::new(ui_v3::ElementType::Div, None)
        .flex()
        .flex_direction(ui_v3::FlexDirection::Column)
        .width_full()
}

fn row() -> ui_v3::Element {
    ui_v3::Element::new(ui_v3::ElementType::Div, None)
        .flex()
        .flex_direction(ui_v3::FlexDirection::Row)
        .width_full()
}

// 复制图标
const COPY_ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>"#;

fn copy_btn(hovered: bool) -> ui_v3::Element {
    ui_v3::Element::new(ui_v3::ElementType::Button, None)
        .without_default_styles()
        .on(ui_v3::Event::Click, EVENT_COPY_LOG)
        .on(ui_v3::Event::MouseEnter, EVENT_COPY_LOG)
        .on(ui_v3::Event::MouseLeave, EVENT_MOUSE_LEAVE)
        .width(28)
        .height(28)
        .radius(8)
        .flex()
        .align_center()
        .justify_center()
        .bg(if hovered { BTN_HOVER } else { BTN_BG })
        .child(
            ui_v3::Element::new(ui_v3::ElementType::Svg, Some(COPY_ICON))
                .width(16)
                .height(16)
                .text_color(TEXT),
        )
}

fn log(line: &str) {
    let mut s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    s.log.push(line.to_string());
    if s.log.len() > MAX_LOG {
        let overflow = s.log.len() - MAX_LOG;
        s.log.drain(0..overflow);
    }
    s.pending_scroll = true;
}

/// render 为同步宿主调用，重绘无需 async
fn redraw() {
    let mut s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(root) = &s.root {
        let ui = build_ui(&s);
        psys_host::ui_v3::render(root, ui);
        s.pending_scroll = false;
    }
}

/// UI 事件分发（由 on_ui_event_v3 通过 block_on 同步等待）
pub async fn handle_ui_event(evtype: ui_v3::Event, event_id: &str, _payload: &str) {
    tracing::info!("ui event: event_id={}, event={:?}", event_id, evtype);
    match evtype {
        ui_v3::Event::Click => match event_id {
            EVENT_PICK_FILE => pick_file().await,
            EVENT_SEND_TO_WATCH => send_to_watch().await,
            EVENT_COPY_LOG => copy_log().await,
            _ => {}
        },
        ui_v3::Event::MouseEnter => {
            set_hovered(Some(event_id.to_string()));
            redraw();
        }
        ui_v3::Event::MouseLeave => {
            set_hovered(None);
            redraw();
        }
        _ => {}
    }
}

fn set_hovered(hovered: Option<String>) {
    let mut s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    s.hovered = hovered;
}

async fn refresh_device() {
    let devices = psys_host::device::get_connected_device_list().await;
    let (name, addr) = devices
        .first()
        .map(|d| (d.name.clone(), d.addr.clone()))
        .unwrap_or_default();
    let changed = {
        let mut s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let changed = s.device_addr != addr;
        s.device_addr = addr.clone();
        changed
    };
    if changed {
        if addr.is_empty() {
            let all = psys_host::device::get_device_list().await;
            let names: Vec<&str> = all.iter().map(|d| d.name.as_str()).collect();
            let msg = if names.is_empty() {
                "未检测到设备（若 AstroBox 已连接，请检查插件是否已授权设备权限）".to_string()
            } else {
                format!("未检测到在线设备；已配对：{}", names.join("、"))
            };
            log(&msg);
        } else {
            log(&format!("已连接设备：{}", name));
        }
    }
    redraw();
}

fn parse_file(file_name: &str, text: &str) -> Result<Value, String> {
    let parsed = if file_name.ends_with(".wakeup_schedule") {
        parser::convert_wakeup_schedule_to_json(text)?
    } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
        parser::convert_cses_yaml_to_json(text)?
    } else {
        serde_json::from_str(text).map_err(|e| format!("配置文件不是有效 JSON: {}", e))?
    };
    if let Some(err) = parser::validate_schedule_config(&parsed) {
        return Err(err);
    }
    Ok(parser::sanitize_schedule_payload(&parsed))
}

async fn pick_file() {
    let result = psys_host::dialog::pick_file(
        &dialog::PickConfig {
            read: true,
            copy_to: None,
        },
        &dialog::FilterConfig {
            multiple: false,
            extensions: vec![],
            default_directory: String::new(),
            default_file_name: String::new(),
        },
    )
    .await;
    if result.name.is_empty() {
        log("未选择文件");
        redraw();
        return;
    }
    let file_name = result.name.clone();
    let text = String::from_utf8_lossy(&result.data).into_owned();
    match parse_file(&file_name, &text) {
        Ok(parsed) => {
            let courses = parsed.get("courses").and_then(Value::as_array).map_or(0, Vec::len);
            let slots = parsed.get("timeSlots").and_then(Value::as_array).map_or(0, Vec::len);
            let mut s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            s.file_name = file_name.clone();
            s.parsed = Some(parsed);
            drop(s);
            log(&format!("解析成功：{}（{} 门课程 / {} 个节次）", file_name, courses, slots));
        }
        Err(e) => log(&format!("解析失败：{}", e)),
    }
    redraw();
}

async fn send_to_watch() {
    let (addr, parsed) = {
        let s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (s.device_addr.clone(), s.parsed.clone())
    };
    if addr.is_empty() {
        log("同步失败：未连接到设备");
        redraw();
        return;
    }
    let payload = match parsed {
        Some(p) => serde_json::to_string(&p).unwrap_or_default(),
        None => {
            log("同步失败：请先选择并解析配置文件");
            redraw();
            return;
        }
    };

    // 尽力拉起快应用确保就绪
    if let Ok(apps) = psys_host::thirdpartyapp::get_thirdparty_app_list(&addr).await
        && let Some(app) = apps.iter().find(|a| a.package_name == PKG_NAME)
    {
        let _ = psys_host::thirdpartyapp::launch_qa(&addr, app, ENTRY_PAGE).await;
    }

    match psys_host::interconnect::send_qaic_message(&addr, PKG_NAME, &payload).await {
        Ok(()) => log("配置已发送到手环"),
        Err(()) => log("配置发送失败：请检查设备连接与互联互通权限"),
    }
    redraw();
}

async fn copy_log() {
    let text = {
        let s = state().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        s.log.join("\n")
    };
    if !text.is_empty() {
        let _ = psys_host::clipboard::write_text(&text).await;
    }
}

fn build_ui(s: &UiState) -> ui_v3::Element {
    // 首行：未选择文件时显示支持提示，选择后显示文件名
    let (file_label, file_color) = if s.file_name.is_empty() {
        ("支持拾光课程表/WakeUp课程表/CSES 配置文件", MUTED)
    } else {
        (s.file_name.as_str(), TEXT)
    };

    // 次行：选择文件 + 推送按钮
    let buttons = row().gap(8)
        .child(btn(
            "选择配置文件",
            EVENT_PICK_FILE,
            s.hovered.as_deref() == Some(EVENT_PICK_FILE),
        ))
        .child(btn(
            "推送到手环",
            EVENT_SEND_TO_WATCH,
            s.hovered.as_deref() == Some(EVENT_SEND_TO_WATCH),
        ));

    // 日志卡片
    let header = row()
        .align_center()
        .gap(8)
        .child(p("日志").size(18).text_color(MUTED).flex_grow(1.0))
        .child(copy_btn(s.hovered.as_deref() == Some(EVENT_COPY_LOG)));

    let mut log_scroll = ui_v3::Element::new(ui_v3::ElementType::ScrollArea, None)
        .width_full()
        .height(LOG_HEIGHT);
    if s.pending_scroll {
        log_scroll = log_scroll.scroll_top(LOG_SCROLL_MAX);
    }
    for line in &s.log {
        log_scroll = log_scroll.child(p(line).size(13).text_color(LOG_C).margin_bottom(2));
    }
    let log_card = col()
        .radius(RADIUS)
        .padding(12)
        .gap(8)
        .bg(CARD_BG)
        .child(header)
        .child(log_scroll);

    col()
        .padding(20)
        .gap(16)
        .child(p(file_label).size(16).text_color(file_color))
        .child(buttons)
        .child(log_card)
}

pub async fn render_main_ui(element_id: &str) {
    state().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).root = Some(element_id.to_string());
    redraw();
}

pub async fn refresh_device_status() {
    refresh_device().await;
}
