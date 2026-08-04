use astrobox_ng_wit::astrobox::psys_host::ui_v3;
use std::sync::{Mutex, OnceLock};

pub const EVENT_SYNC: &str = "bandtotp_sync";
pub const EVENT_PICK_FILE: &str = "bandtotp_pick_file";
pub const EVENT_LAUNCH_APP: &str = "bandtotp_launch_app";

pub const ID_URI_INPUT: &str = "bandtotp_uri_input";
pub const ID_STATUS_TEXT: &str = "bandtotp_status";
const BANDTOTP_PACKAGE_NAME: &str = "com.lst.bandtotp";

struct UiState {
    root_element_id: Option<String>,
    last_status: String,
    uri_text: String,
}

static UI_STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

fn ui_state() -> &'static Mutex<UiState> {
    UI_STATE.get_or_init(|| {
        Mutex::new(UiState {
            root_element_id: None,
            last_status: "准备就绪".to_string(),
            uri_text: String::new(),
        })
    })
}

pub fn set_root_id(id: String) {
    let mut state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.root_element_id = Some(id);
}

pub fn render_main_ui(element_id: &str) {
    let state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ui_v3::render(element_id, build_ui(&state));
}

pub async fn handle_ui_event(evtype: ui_v3::Event, event_id: &str, payload: &str) {
    match evtype {
        ui_v3::Event::Click => match event_id {
            EVENT_SYNC => handle_sync().await,
            EVENT_PICK_FILE => handle_pick_file().await,
            EVENT_LAUNCH_APP => handle_launch_app().await,
            _ => {}
        },
        ui_v3::Event::Input | ui_v3::Event::Change | ui_v3::Event::Blur => {
            if let Some(value) = parse_input_value(payload, event_id) {
                if event_id == ID_URI_INPUT {
                    update_uri_text(&value);
                }
            }
        }
        _ => {
            tracing::warn!(
                "unhandled event: evtype={:?}, event_id={}",
                evtype,
                event_id
            );
        }
    }
}

fn parse_input_value(payload: &str, event_id: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    value
        .get("value")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("inputs")
                .and_then(|inputs| inputs.get(event_id))
                .and_then(|v| v.as_str())
        })
        .map(str::to_string)
}

fn update_uri_text(text: &str) {
    let mut state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.uri_text = text.to_string();
}

async fn handle_sync() {
    let text = {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.uri_text.clone()
    };

    update_status("正在同步到手环...");

    let entries = crate::sync::parse_totp_text(&text);
    if entries.is_empty() {
        update_status("没有解析到有效的 otpauth://totp URI");
        return;
    }

    let payload = crate::sync::SyncPayload { list: entries };
    let json = payload.to_json();

    match crate::sync::send_payload(&json, BANDTOTP_PACKAGE_NAME).await {
        Ok(msg) => update_status(&format!("同步成功：{}", msg)),
        Err(e) => update_status(&format!("同步失败：{}", e)),
    }
}

async fn handle_pick_file() {
    tracing::info!("handle_pick_file");
    update_status("正在选择文件...");

    match crate::sync::pick_totp_file().await {
        Ok(text) => {
            let count = crate::sync::parse_totp_text(&text).len();
            {
                let mut state = ui_state()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.uri_text = text;
            }
            update_status(&format!("已选择文件，解析出 {} 条记录", count));
        }
        Err(e) => update_status(&format!("选择文件失败：{}", e)),
    }
}

async fn handle_launch_app() {
    update_status("正在打开手环应用...");

    match crate::sync::launch_band_totp(BANDTOTP_PACKAGE_NAME).await {
        Ok(msg) => update_status(&msg),
        Err(e) => update_status(&format!("打开失败：{}", e)),
    }
}

fn build_ui(state: &UiState) -> ui_v3::Element {
    let title = ui_v3::Element::new(ui_v3::ElementType::P, Some("BandTOTP 同步"))
        .size(24)
        .margin_bottom(8);

    let package_hint = ui_v3::Element::new(
        ui_v3::ElementType::P,
        Some(&format!("固定同步到手环应用：{}", BANDTOTP_PACKAGE_NAME)),
    )
    .size(14)
    .margin_bottom(8);

    let uri_input = ui_v3::Element::new(ui_v3::ElementType::Textarea, Some(&state.uri_text))
        .prop("id", ID_URI_INPUT)
        .prop("placeholder", "每行粘贴一个 otpauth://totp/... URI")
        .width_full()
        .height(160)
        .margin_bottom(8)
        .on(ui_v3::Event::Input, ID_URI_INPUT)
        .on(ui_v3::Event::Change, ID_URI_INPUT)
        .on(ui_v3::Event::Blur, ID_URI_INPUT);

    let btn_row = ui_v3::Element::new(ui_v3::ElementType::Div, None)
        .flex()
        .flex_direction(ui_v3::FlexDirection::Row)
        .gap(8)
        .width_full()
        .margin_bottom(8)
        .child(
            ui_v3::Element::new(ui_v3::ElementType::Button, Some("选择文件"))
                .on(ui_v3::Event::Click, EVENT_PICK_FILE),
        )
        .child(
            ui_v3::Element::new(ui_v3::ElementType::Button, Some("打开手环应用"))
                .on(ui_v3::Event::Click, EVENT_LAUNCH_APP),
        )
        .child(
            ui_v3::Element::new(ui_v3::ElementType::Button, Some("同步到手环"))
                .bg("#00AA66")
                .on(ui_v3::Event::Click, EVENT_SYNC),
        );

    let status_card = ui_v3::Element::new(ui_v3::ElementType::Card, None)
        .width_full()
        .padding(12)
        .bg("#1e1e1e")
        .child(ui_v3::Element::new(ui_v3::ElementType::P, Some("状态")).size(18))
        .child(
            ui_v3::Element::new(ui_v3::ElementType::P, Some(&state.last_status))
                .prop("id", ID_STATUS_TEXT)
                .size(14),
        );

    ui_v3::Element::new(ui_v3::ElementType::Div, None)
        .flex()
        .flex_direction(ui_v3::FlexDirection::Column)
        .width_full()
        .padding(16)
        .child(title)
        .child(package_hint)
        .child(uri_input)
        .child(btn_row)
        .child(status_card)
}

fn refresh_ui() {
    let root_id = {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id.clone()
    };

    if let Some(root_id) = root_id {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ui_v3::render(&root_id, build_ui(&state));
    }
}

fn update_status(message: &str) {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.last_status = message.to_string();
    }
    refresh_ui();
}
