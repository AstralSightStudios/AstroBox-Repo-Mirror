use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use astrobox_ng_wit::FutureReader;
use astrobox_ng_wit::astrobox::psys_host::{self, dialog, ui};
use astrobox_ng_wit::exports::astrobox::psys_plugin::{
    event::{self, EventType},
    lifecycle,
};
use serde_json::{Value, json};

mod logger;

const PACKAGE_NAME: &str = "com.cxkpro.qrcodetool";
const EVENT_REFRESH: &str = "qt_refresh";
const EVENT_TITLE: &str = "qt_title";
const EVENT_NOTE: &str = "qt_note";
const EVENT_CONTENT: &str = "qt_content";
const EVENT_PICK_QR: &str = "qt_pick_qr";
const EVENT_PASTE_WEB: &str = "qt_paste_web";
const EVENT_SEND: &str = "qt_send";

#[derive(Clone, Default)]
struct State {
    element_id: Option<String>,
    device_addr: String,
    device_name: String,
    status: String,
    title: String,
    note: String,
    content: String,
    registered_recv: bool,
    last_action_id: String,
    last_action_at_ms: u128,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            status: "请先点击“刷新设备”并授予设备权限".into(),
            ..State::default()
        })
    })
}

fn snapshot() -> State {
    state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn update(action: impl FnOnce(&mut State)) {
    action(&mut state().lock().unwrap_or_else(|error| error.into_inner()));
}

fn text(content: &str, size: u32, color: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(content))
        .size(size)
        .text_color(color)
}

fn button(label: &str, id: &str, color: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(label))
        .width_full()
        .padding(12)
        .margin_top(8)
        .radius(14)
        .bg(color)
        .text_color("#FFFFFF")
        .on(ui::Event::Click, id)
        .on(ui::Event::PointerUp, id)
}

fn input(value: &str, id: &str, height: u32) -> ui::Element {
    ui::Element::new(ui::ElementType::Input, Some(value))
        .width_full()
        .height(height)
        .margin_top(7)
        .padding(10)
        .radius(12)
        .bg("#171717")
        .border(1, "#484848")
        .text_color("#FFFFFF")
        .on(ui::Event::Input, id)
        .on(ui::Event::Change, id)
}

fn render() {
    let current = snapshot();
    let Some(element_id) = current.element_id else {
        return;
    };
    let device = if current.device_addr.is_empty() {
        "尚未选择设备".to_string()
    } else {
        format!("{}\n{}", current.device_name, current.device_addr)
    };
    let content_status = if current.content.is_empty() {
        "尚未导入二维码内容"
    } else {
        "二维码内容已就绪"
    };
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .padding(14)
        .bg("#090909")
        .child(text("QT Vela", 26, "#FFFFFF"))
        .child(text("二维码同步器", 14, "#AAAAAE"))
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Column)
                .width_full()
                .padding(14)
                .margin_top(12)
                .radius(18)
                .bg("#292929")
                .child(text("当前设备", 18, "#FFFFFF"))
                .child(text(&device, 14, "#AAAAAE"))
                .child(text(&current.status, 14, "#AAAAAE")),
        )
        .child(button("刷新设备", EVENT_REFRESH, "#414141"))
        .child(text("标题（必填）", 15, "#FFFFFF"))
        .child(input(&current.title, EVENT_TITLE, 48))
        .child(text("备注（可不填）", 15, "#FFFFFF"))
        .child(input(&current.note, EVENT_NOTE, 48))
        .child(text("二维码内容（必填）", 15, "#FFFFFF"))
        .child(input(&current.content, EVENT_CONTENT, 72))
        .child(button("选择二维码图片并识别", EVENT_PICK_QR, "#414141"))
        .child(button("一键导入网页复制的数据", EVENT_PASTE_WEB, "#414141"))
        .child(text(content_status, 13, "#AAAAAE"))
        .child(button("同步到手表", EVENT_SEND, "#1265E8"));
    ui::render(&element_id, root);
}

fn parse_input_value(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<Value>(payload) {
        json.get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    } else {
        payload.to_string()
    }
}

fn handle_input(event_id: &str, payload: &str) {
    let value = parse_input_value(payload);
    update(|state| match event_id {
        EVENT_TITLE => state.title = value,
        EVENT_NOTE => state.note = value,
        EVENT_CONTENT => state.content = value,
        _ => {}
    });
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn is_duplicate_action(event_id: &str) -> bool {
    let now = current_millis();
    let mut duplicated = false;
    update(|state| {
        duplicated =
            state.last_action_id == event_id && now.saturating_sub(state.last_action_at_ms) < 500;
        if !duplicated {
            state.last_action_id = event_id.to_string();
            state.last_action_at_ms = now;
        }
    });
    duplicated
}

fn mark_action_completed(event_id: &str) {
    let now = current_millis();
    update(|state| {
        state.last_action_id = event_id.to_string();
        state.last_action_at_ms = now;
    });
}

fn is_text_input_event(event_id: &str) -> bool {
    matches!(event_id, EVENT_TITLE | EVENT_NOTE | EVENT_CONTENT)
}

fn is_known_action(event_id: &str) -> bool {
    matches!(
        event_id,
        EVENT_REFRESH | EVENT_PICK_QR | EVENT_PASTE_WEB | EVENT_SEND
    )
}

fn pending_message(event_id: &str) -> &'static str {
    match event_id {
        EVENT_REFRESH => "正在申请权限并刷新设备…",
        EVENT_PICK_QR => "正在打开二维码图片…",
        EVENT_PASTE_WEB => "正在打开网页数据导入框…",
        EVENT_SEND => "正在发送二维码到手表…",
        _ => "正在处理…",
    }
}

async fn refresh() -> String {
    // device 权限由宿主在此 API 首次调用时申请；不在 on_load 中提前触发。
    let devices = psys_host::device::get_connected_device_list().await;
    if let Some(device) = devices.first() {
        let registered =
            psys_host::register::register_interconnect_recv(&device.addr, PACKAGE_NAME)
                .await
                .is_ok();
        update(|state| {
            state.device_addr = device.addr.clone();
            state.device_name = device.name.clone();
            state.registered_recv = registered;
            state.status = if registered {
                "设备已连接，可以同步".into()
            } else {
                "设备已连接，但通信接收注册失败".into()
            };
        });
        render();
        "refresh-ok".into()
    } else {
        update(|state| {
            state.device_addr.clear();
            state.device_name.clear();
            state.registered_recv = false;
            state.status = "未发现已连接手表，请检查连接和 device 权限".into();
        });
        render();
        "no-device".into()
    }
}

fn decode_qr(data: &[u8]) -> Result<String, String> {
    let image = image::load_from_memory(data)
        .map_err(|_| "无法读取图片，请选择 PNG、JPEG 或 WebP".to_string())?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    for grid in prepared.detect_grids() {
        if let Ok((_meta, content)) = grid.decode() {
            if !content.is_empty() {
                return Ok(content);
            }
        }
    }
    Err("图片中没有识别到二维码".to_string())
}

async fn pick_qr() -> String {
    let picked = dialog::pick_file(
        &dialog::PickConfig {
            read: true,
            copy_to: None,
        },
        &dialog::FilterConfig {
            multiple: false,
            extensions: vec!["png".into(), "jpg".into(), "jpeg".into(), "webp".into()],
            default_directory: "".into(),
            default_file_name: "".into(),
        },
    )
    .await;
    if picked.name.is_empty() {
        return "cancelled".into();
    }
    match decode_qr(&picked.data) {
        Ok(content) => {
            update(|state| {
                state.content = content;
                state.status = "二维码图片识别成功".into();
            });
            render();
            "qr-decoded".into()
        }
        Err(message) => show_error(&message).await,
    }
}

fn parse_web_item(source: &str) -> Result<(String, String, String), String> {
    let parsed: Value =
        serde_json::from_str(source.trim()).map_err(|_| "网页数据不是有效 JSON".to_string())?;
    let item = if parsed.get("type").and_then(Value::as_str) == Some("qt.import") {
        parsed
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .ok_or_else(|| "网页数据中没有二维码".to_string())?
    } else {
        parsed.get("item").unwrap_or(&parsed)
    };
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let note = item
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let content = item
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if title.is_empty() || content.is_empty() {
        return Err("网页数据必须包含标题和二维码内容".to_string());
    }
    Ok((title, note, content))
}

async fn paste_web_data() -> String {
    let result = dialog::show_dialog(
        dialog::DialogType::Input,
        dialog::DialogStyle::Website,
        &dialog::DialogInfo {
            title: "导入网页数据".into(),
            content: "粘贴网页“复制同步数据”生成的内容".into(),
            buttons: vec![
                dialog::DialogButton {
                    id: "cancel".into(),
                    primary: false,
                    content: "取消".into(),
                },
                dialog::DialogButton {
                    id: "import".into(),
                    primary: true,
                    content: "一键导入".into(),
                },
            ],
        },
    )
    .await;
    if result.clicked_btn_id != "import" {
        return "cancelled".into();
    }
    match parse_web_item(&result.input_result) {
        Ok((title, note, content)) => {
            update(|state| {
                state.title = title;
                state.note = note;
                state.content = content;
                state.status = "网页数据已导入，可以同步".into();
            });
            render();
            "web-imported".into()
        }
        Err(message) => show_error(&message).await,
    }
}

async fn send_item() -> String {
    let current = snapshot();
    if current.device_addr.is_empty() {
        return show_error("请先点击刷新设备").await;
    }
    if current.title.trim().is_empty() {
        return show_error("请填写标题").await;
    }
    if current.content.trim().is_empty() {
        return show_error("请选择二维码图片或输入二维码内容").await;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let payload = json!({
        "type": "qt.add",
        "version": 1,
        "requestId": format!("qt-{timestamp}"),
        "item": {
            "id": format!("sync-{timestamp}"),
            "title": current.title.trim(),
            "note": current.note.trim(),
            "content": current.content.trim()
        }
    })
    .to_string();
    match psys_host::interconnect::send_qaic_message(&current.device_addr, PACKAGE_NAME, &payload)
        .await
    {
        Ok(()) => {
            update(|state| state.status = "已发送，等待手表保存".into());
            render();
            "add-sent".into()
        }
        Err(()) => show_error("发送失败，请在手表打开 QT Vela 同步页面").await,
    }
}

fn extract_payload_text(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<Value>(payload) {
        if let Some(text) = json.get("payloadText").and_then(Value::as_str) {
            return text.into();
        }
        if let Some(inner) = json.get("payload") {
            return inner
                .as_str()
                .map_or_else(|| inner.to_string(), str::to_string);
        }
    }
    payload.into()
}

fn handle_message(payload: &str) {
    let text = extract_payload_text(payload);
    let Ok(message) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    if message.get("type").and_then(Value::as_str) == Some("qt.syncResult") {
        let success = message
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        update(|state| {
            state.status = if success {
                state.title.clear();
                state.note.clear();
                state.content.clear();
                "手表已保存二维码".into()
            } else {
                "手表保存二维码失败".into()
            };
        });
        render();
    }
}

async fn show_error(message: &str) -> String {
    update(|state| state.status = message.into());
    render();
    let _ = dialog::show_dialog(
        dialog::DialogType::Alert,
        dialog::DialogStyle::System,
        &dialog::DialogInfo {
            title: "QT Vela".into(),
            content: message.into(),
            buttons: vec![dialog::DialogButton {
                id: "ok".into(),
                primary: true,
                content: "确定".into(),
            }],
        },
    )
    .await;
    "error".into()
}

fn handle_ui_action_sync(event_id: &str) -> String {
    match event_id {
        EVENT_REFRESH => astrobox_ng_wit::block_on(async { refresh().await }),
        EVENT_PICK_QR => astrobox_ng_wit::block_on(async { pick_qr().await }),
        EVENT_PASTE_WEB => astrobox_ng_wit::block_on(async { paste_web_data().await }),
        EVENT_SEND => astrobox_ng_wit::block_on(async { send_item().await }),
        _ => "ignored".into(),
    }
}

struct QtPlugin;

impl event::Guest for QtPlugin {
    fn on_event(event_type: EventType, payload: String) -> FutureReader<String> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(String::new);
        if matches!(event_type, EventType::InterconnectMessage) {
            handle_message(&payload);
        }
        astrobox_ng_wit::spawn(async move {
            let _ = writer.write("accepted".into()).await;
        });
        reader
    }

    fn on_ui_event(event_id: String, event: event::Event, payload: String) -> FutureReader<String> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(|| "".to_string());
        let is_text_input = matches!(event, event::Event::Input | event::Event::Change)
            && is_text_input_event(&event_id);
        if is_text_input {
            handle_input(&event_id, &payload);
            astrobox_ng_wit::spawn(async move {
                let _ = writer.write("accepted".to_string()).await;
            });
            return reader;
        }

        let is_action = matches!(event, event::Event::Click | event::Event::PointerUp)
            && is_known_action(&event_id);
        if !is_action {
            astrobox_ng_wit::spawn(async move {
                let _ = writer.write("unknown-ui-event".to_string()).await;
            });
            return reader;
        }

        if is_duplicate_action(&event_id) {
            astrobox_ng_wit::spawn(async move {
                let _ = writer.write("accepted".to_string()).await;
            });
            return reader;
        }

        update(|state| state.status = pending_message(&event_id).to_string());
        render();

        // 与 Shell++ 相同：Host API 在 UI 回调内用 block_on 执行，
        // spawn 只用于完成 FutureReader 的 writer。
        let result = handle_ui_action_sync(&event_id);
        // Dialog 会阻塞到用户输入结束，宿主随后才派发 POINTER-UP。
        // 以操作完成时间重置去重窗口，防止同一次点击再次打开弹窗。
        mark_action_completed(&event_id);
        if result == "ignored" {
            update(|state| state.status = result);
            render();
        }

        astrobox_ng_wit::spawn(async move {
            let _ = writer.write("accepted".to_string()).await;
        });
        reader
    }

    fn on_ui_render(element_id: String) -> FutureReader<()> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<()>(|| ());
        update(|state| state.element_id = Some(element_id));
        render();
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

impl lifecycle::Guest for QtPlugin {
    fn on_load() {
        logger::init();
        tracing::info!("QT Vela AstroBox plugin loaded; waiting for user refresh");
    }
}

astrobox_ng_wit::export!(QtPlugin with_types_in astrobox_ng_wit);
