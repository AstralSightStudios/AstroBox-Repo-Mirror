use crate::astrobox::psys_host::ui as ui_old;
use crate::astrobox::psys_host::ui_v3 as ui;
use std::sync::{Mutex, OnceLock};

use crate::sync::{self, HostEntry};

pub const EVENT_LOAD_FROM_WATCH: &str = "load_from_watch";
pub const EVENT_SAVE_TO_WATCH: &str = "save_to_watch";
pub const EVENT_ADD_HOST: &str = "add_host";
pub const EVENT_CONFIRM_SAVE: &str = "confirm_save";
pub const EVENT_CANCEL_SAVE: &str = "cancel_save";
pub const EVENT_DELETE_ENTRY: &str = "delete_entry_";
pub const EVENT_EDIT_ENTRY: &str = "edit_entry_";
pub const EVENT_SAVE_EDIT: &str = "save_edit";
pub const EVENT_CANCEL_EDIT: &str = "cancel_edit";
pub const EVENT_SAVE_ADD: &str = "save_add";
pub const EVENT_CANCEL_ADD: &str = "cancel_add";

pub const EDIT_NAME: &str = "edit_name";
pub const EDIT_URL: &str = "edit_url";
pub const EDIT_ENCRYPT_MODE: &str = "edit_encryptMode";
pub const EDIT_SECRET: &str = "edit_secret";
pub const EDIT_SM4: &str = "edit_sm4KeyHex";

pub const ADD_NAME: &str = "add_name";
pub const ADD_URL: &str = "add_url";
pub const ADD_ENCRYPT_MODE: &str = "add_encryptMode";
pub const ADD_SECRET: &str = "add_secret";
pub const ADD_SM4: &str = "add_sm4KeyHex";

pub const EVENT_CANCEL_DELETE_ENTRY: &str = "cancel_delete_entry";
pub const EVENT_CONFIRM_DELETE_ENTRY: &str = "confirm_delete_entry";

pub const ID_STATUS_TEXT: &str = "status_text";

struct UiState {
    root_element_id: Option<String>,
    last_status: String,
    show_save_confirm: bool,
    save_confirm_desc: String,
    show_edit_form: bool,
    edit_form_index: Option<usize>,
    edit_form_data: HostEntry,
    show_add_form: bool,
    add_form_data: HostEntry,
    show_delete_confirm: bool,
    delete_confirm_index: Option<usize>,
}

static UI_STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

fn default_host_entry() -> HostEntry {
    HostEntry {
        id: String::new(),
        name: String::new(),
        url: String::new(),
        secret: String::new(),
        encrypt_mode: 0,
        sm4_key_hex: None,
        connected: None,
        created_at: None,
    }
}

fn ui_state() -> &'static Mutex<UiState> {
    UI_STATE.get_or_init(|| {
        Mutex::new(UiState {
            root_element_id: None,
            last_status: "等待操作".to_string(),
            show_save_confirm: false,
            save_confirm_desc: String::new(),
            show_edit_form: false,
            edit_form_index: None,
            edit_form_data: default_host_entry(),
            show_add_form: false,
            add_form_data: default_host_entry(),
            show_delete_confirm: false,
            delete_confirm_index: None,
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
    ui::render(element_id, build_ui(&state));
}

pub fn refresh_ui() {
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
        ui::render(&root_id, build_ui(&state));
    }
}

pub fn is_dialog_open() -> bool {
    let state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.show_edit_form
        || state.show_add_form
        || state.show_save_confirm
        || state.show_delete_confirm
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

pub fn update_status_direct(message: &str) {
    let mut state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_status = message.to_string();
}

pub async fn handle_ui_event(evtype: ui_old::Event, event_id: &str, payload: &str) {
    let evtype = match evtype {
        ui_old::Event::Click => ui::Event::Click,
        ui_old::Event::Hover => ui::Event::Hover,
        ui_old::Event::Change => ui::Event::Change,
        ui_old::Event::Input => ui::Event::Input,
        ui_old::Event::Focus => ui::Event::Focus,
        ui_old::Event::Blur => ui::Event::Blur,
        ui_old::Event::MouseEnter => ui::Event::MouseEnter,
        ui_old::Event::MouseLeave => ui::Event::MouseLeave,
        ui_old::Event::PointerDown => ui::Event::PointerDown,
        ui_old::Event::PointerUp => ui::Event::PointerUp,
        ui_old::Event::PointerMove => ui::Event::PointerMove,
    };
    match evtype {
        ui::Event::Click => match event_id {
            EVENT_LOAD_FROM_WATCH => handle_load_from_watch(),
            EVENT_SAVE_TO_WATCH => handle_save_to_watch(),
            EVENT_CONFIRM_SAVE => handle_confirm_save(),
            EVENT_CANCEL_SAVE => handle_cancel_save(),
            EVENT_ADD_HOST => handle_add_host(),
            EVENT_SAVE_EDIT => handle_save_edit(),
            EVENT_CANCEL_EDIT => handle_cancel_edit(),
            EVENT_SAVE_ADD => handle_save_add(),
            EVENT_CANCEL_ADD => handle_cancel_add(),
            EVENT_CONFIRM_DELETE_ENTRY => handle_confirm_delete_entry(),
            EVENT_CANCEL_DELETE_ENTRY => handle_cancel_delete_entry(),
            _ if event_id.starts_with(EVENT_DELETE_ENTRY) => {
                if let Ok(idx) = event_id
                    .trim_start_matches(EVENT_DELETE_ENTRY)
                    .parse::<usize>()
                {
                    handle_delete_entry(idx);
                }
            }
            _ if event_id.starts_with(EVENT_EDIT_ENTRY) => {
                if let Ok(idx) = event_id
                    .trim_start_matches(EVENT_EDIT_ENTRY)
                    .parse::<usize>()
                {
                    handle_edit_entry(idx);
                }
            }
            _ if event_id.starts_with("edit_encryptMode_")
                || event_id.starts_with("add_encryptMode_") =>
            {
                handle_encrypt_mode_select(event_id);
            }
            _ => {}
        },
        ui::Event::Change => {
            if event_id.starts_with("edit_") || event_id.starts_with("add_") {
                update_form_field(event_id, payload);
            }
        }
        ui::Event::Input | ui::Event::Blur => {
            if event_id.starts_with("edit_") || event_id.starts_with("add_") {
                update_form_field(event_id, payload);
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

fn update_form_field(event_id: &str, payload: &str) {
    if let Some(value) = parse_input_value(payload, event_id) {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let is_edit = event_id.starts_with("edit_");
        let entry = if is_edit {
            &mut state.edit_form_data
        } else {
            &mut state.add_form_data
        };
        let field_name = if is_edit {
            event_id
        } else {
            &event_id.replacen("add_", "edit_", 1)
        };
        match field_name {
            EDIT_NAME => entry.name = value,
            EDIT_URL => entry.url = value,
            EDIT_SECRET => entry.secret = value,
            EDIT_SM4 => {
                let normalized: String = value.chars()
                    .filter(|c| !c.is_whitespace())
                    .map(|c| c.to_ascii_uppercase())
                    .collect();
                entry.sm4_key_hex = if normalized.is_empty() { None } else { Some(normalized) }
            }
            EDIT_ENCRYPT_MODE => {
                entry.encrypt_mode = value.parse::<u32>().unwrap_or(0);
            }
            _ => {}
        }
    }
}

fn handle_encrypt_mode_select(event_id: &str) {
    let mut state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let is_edit = event_id.starts_with("edit_");
    let prefix = if is_edit {
        "edit_encryptMode_"
    } else {
        "add_encryptMode_"
    };
    let mode_str = event_id.trim_start_matches(prefix);
    let mode = mode_str.parse::<u32>().unwrap_or(0);
    let entry = if is_edit {
        &mut state.edit_form_data
    } else {
        &mut state.add_form_data
    };
    entry.encrypt_mode = mode;
    drop(state);
    refresh_ui();
}

fn handle_load_from_watch() {
    if sync::is_loading() {
        update_status("正在从手表加载数据中，请稍候...");
        return;
    }

    update_status("正在连接设备...");
    wit_bindgen::spawn(async move {
        let device_addr = match crate::device::check_device().await {
            Some(addr) => addr,
            None => {
                update_status("未检测到已连接设备");
                return;
            }
        };

        let pkg_name = match crate::device::resolve_pkg_name(&device_addr).await {
            Some(name) => name,
            None => {
                update_status("未找到 SmsForwarder Client 应用");
                return;
            }
        };

        update_status("正在打开手表应用...");
        if let Err(e) = crate::device::launch_and_wait(&device_addr, &pkg_name).await {
            update_status(&format!("打开应用失败: {}", e));
            return;
        }

        update_status("应用已就绪，请求主机列表...");
        if let Err(e) = sync::request_list_hosts().await {
            update_status(&format!("发送请求失败: {}", e));
        }
    });
}

fn handle_save_to_watch() {
    let (upsert, remove) = sync::get_sync_delta();
    if upsert.is_empty() && remove.is_empty() {
        update_status("没有增量数据需要同步");
        return;
    }

    let mut desc_lines: Vec<String> = Vec::new();
    if !upsert.is_empty() {
        desc_lines.push(format!("新增/修改 {} 项:", upsert.len()));
        for e in &upsert {
            desc_lines.push(format!("  {}", e.name));
        }
    }
    if !remove.is_empty() {
        if !desc_lines.is_empty() {
            desc_lines.push(String::new());
        }
        desc_lines.push(format!("删除 {} 项", remove.len()));
    }

    let desc = desc_lines.join("\n");

    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_save_confirm = true;
        state.save_confirm_desc = desc;
    }
    refresh_ui();
}

fn handle_confirm_save() {
    let (upsert, remove) = sync::get_sync_delta();
    if upsert.is_empty() && remove.is_empty() {
        update_status("没有增量数据需要同步");
        {
            let mut state = ui_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.show_save_confirm = false;
        }
        refresh_ui();
        return;
    }

    update_status("正在保存增量修改至手表...");

    wit_bindgen::spawn(async move {
        match sync::send_sync_delta().await {
            Ok(msg) => {
                update_status(&format!("保存成功: {}", msg));
            }
            Err(e) => update_status(&format!("保存失败: {}", e)),
        }

        {
            let mut state = ui_state()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.show_save_confirm = false;
        }
        refresh_ui();
    });
}

fn handle_cancel_save() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_save_confirm = false;
    }
    refresh_ui();
}

fn handle_delete_entry(index: usize) {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_delete_confirm = true;
        state.delete_confirm_index = Some(index);
    }
    refresh_ui();
}

fn handle_confirm_delete_entry() {
    let index = {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.delete_confirm_index
    };

    if let Some(idx) = index {
        sync::delete_local_entry(idx);
    }

    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_delete_confirm = false;
        state.delete_confirm_index = None;
        if let Some(idx) = index {
            if state.edit_form_index == Some(idx) {
                state.show_edit_form = false;
                state.edit_form_index = None;
                state.edit_form_data = default_host_entry();
            }
        }
    }
    update_status("已删除本地条目（需保存修改至手表生效）");
}

fn handle_cancel_delete_entry() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_delete_confirm = false;
        state.delete_confirm_index = None;
    }
    refresh_ui();
}

fn handle_edit_entry(index: usize) {
    if let Some(entry) = sync::get_entry_by_index(index) {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_edit_form = true;
        state.edit_form_index = Some(index);
        state.edit_form_data = entry;
    }
    refresh_ui();
}

fn handle_save_edit() {
    let (index, entry) = {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.edit_form_index, state.edit_form_data.clone())
    };
    if let Some(idx) = index {
        if let Some(old) = sync::get_entry_by_index(idx) {
            sync::update_local_entry_by_url(&old.url, entry);
            update_status("已保存编辑（需保存修改至手表生效）");
        }
    }
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_edit_form = false;
        state.edit_form_index = None;
        state.edit_form_data = default_host_entry();
    }
    refresh_ui();
}

fn handle_cancel_edit() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_edit_form = false;
        state.edit_form_index = None;
        state.edit_form_data = default_host_entry();
    }
    refresh_ui();
}

fn handle_add_host() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_add_form = true;
        state.add_form_data = default_host_entry();
    }
    refresh_ui();
}

fn handle_save_add() {
    let entry = {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut e = state.add_form_data.clone();
        if e.id.is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            e.id = format!("host_{}_{}", now, rand_simple());
            e.created_at = Some(now);
        }
        e
    };
    sync::add_local_entry(entry);
    update_status("已添加主机（需保存修改至手表生效）");
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_add_form = false;
        state.add_form_data = default_host_entry();
    }
    refresh_ui();
}

fn handle_cancel_add() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_add_form = false;
        state.add_form_data = default_host_entry();
    }
    refresh_ui();
}

fn rand_simple() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub fn apply_interconnect_result(result: sync::InterconnectResult) {
    match result {
        sync::InterconnectResult::HostList(entries) => {
            update_status(&format!("已加载 {} 条主机", entries.len()));
        }
        sync::InterconnectResult::OperationResult { message, is_error } => {
            update_status(&message);
            if !is_error {
                tracing::info!("operation success: {}", message);
            }
        }
    }
}

fn build_ui(state: &UiState) -> ui::Element {
    let status = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .padding(10)
        .bg("#1a1a1a")
        .radius(10)
        .margin_bottom(10)
        .child(
            ui::Element::new(ui::ElementType::P, Some(&state.last_status))
                .size(12)
                .text_color("#a0a0a0"),
        );

    let any_dialog = state.show_edit_form
        || state.show_add_form
        || state.show_save_confirm
        || state.show_delete_confirm;

    let mut root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .padding(16)
        .bg("#0f0f0f")
        .child(status);

    if any_dialog {
        if state.show_delete_confirm {
            root = root.child(build_delete_confirm_dialog(state));
        } else if state.show_edit_form {
            root = root.child(build_edit_dialog(state));
        } else if state.show_add_form {
            root = root.child(build_add_dialog(state));
        } else if state.show_save_confirm {
            root = root.child(build_save_confirm_dialog(&state.save_confirm_desc));
        }
    } else {
        root = root.child(build_hosts_tab(state));
    }

    root
}

fn build_hosts_tab(_state: &UiState) -> ui::Element {
    let entries = sync::read_state(|s| s.local_entries.clone());
    let (upsert, remove) = sync::get_sync_delta();
    let pending_count = upsert.len() + remove.len();

    let save_label = if pending_count > 0 {
        format!("保存修改 ({})", pending_count)
    } else {
        "保存修改".to_string()
    };
    let save_bg = if pending_count > 0 {
        "#00AA66"
    } else {
        "#1e1e1e"
    };
    let save_text = if pending_count > 0 {
        "#ffffff"
    } else {
        "#666666"
    };

    let action_bar = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .margin_bottom(12)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .gap(8)
        .child(
            ui::Element::new(ui::ElementType::Button, Some("+ 新建"))
                .bg("#0055AA")
                .text_color("#ffffff")
                .size(13)
                .padding(8)
                .radius(8)
                .flex_grow(1.0)
                .on(ui::Event::Click, EVENT_ADD_HOST),
        )
        .child(
            ui::Element::new(ui::ElementType::Button, Some("拉取数据"))
                .bg("#0090FF")
                .text_color("#ffffff")
                .size(13)
                .padding(8)
                .radius(8)
                .flex_grow(1.0)
                .on(ui::Event::Click, EVENT_LOAD_FROM_WATCH),
        )
        .child(
            ui::Element::new(ui::ElementType::Button, Some(&save_label))
                .bg(save_bg)
                .text_color(save_text)
                .size(13)
                .padding(8)
                .radius(8)
                .flex_grow(1.0)
                .on(ui::Event::Click, EVENT_SAVE_TO_WATCH),
        );

    let mut list = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    if entries.is_empty() {
        list = list.child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .padding(24)
                .align_center()
                .child(
                    ui::Element::new(ui::ElementType::P, Some("暂无主机"))
                        .text_color("#888888")
                        .size(14)
                        .margin_bottom(6),
                )
                .child(
                    ui::Element::new(
                        ui::ElementType::P,
                        Some("点击上方按钮新建或拉取手表数据"),
                    )
                    .text_color("#666666")
                    .size(12),
                ),
        );
    } else {
        let row_parts: Vec<(String, String, HostEntry)> = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let edit_event = format!("{}{}", EVENT_EDIT_ENTRY, i);
                let url_abbrev = if entry.url.chars().count() > 40 {
                    let mut s: String = entry.url.chars().take(40).collect();
                    s.push('…');
                    s
                } else {
                    entry.url.clone()
                };
                (edit_event, url_abbrev, entry.clone())
            })
            .collect();
        for (edit_event, url_abbrev, entry) in row_parts.iter() {
            let (chip_label, chip_bg, chip_color) = match entry.encrypt_mode {
                0 => ("NONE", "#1e1e1e", "#a0a0a0"),
                1 => ("SIGN", "rgba(0,144,255,0.16)", "#0090FF"),
                3 => ("SM4", "rgba(0,170,102,0.16)", "#00AA66"),
                _ => ("?", "#1e1e1e", "#888888"),
            };

            let row = ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .padding(12)
                .bg("#141414")
                .radius(10)
                .margin_bottom(8)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .align_center()
                .on(ui::Event::Click, edit_event)
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .flex()
                        .flex_direction(ui::FlexDirection::Column)
                        .flex_grow(1.0)
                        .child(
                            ui::Element::new(ui::ElementType::P, Some(&entry.name))
                                .size(14)
                                .text_color("#ffffff"),
                        )
                        .child(
                            ui::Element::new(ui::ElementType::P, Some(url_abbrev))
                                .size(12)
                                .text_color("#888888")
                                .margin_top(2),
                        ),
                )
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .padding(6)
                        .radius(6)
                        .bg(chip_bg)
                        .child(
                            ui::Element::new(ui::ElementType::P, Some(chip_label))
                                .size(12)
                                .text_color(chip_color),
                        ),
                );

            list = list.child(row);
        }
    }

    let scroll_area = ui::Element::new(ui::ElementType::ScrollArea, None)
        .width_full()
        .child(list);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(action_bar)
        .child(scroll_area)
}

fn build_save_confirm_dialog(desc: &str) -> ui::Element {
    let content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("保存修改到手表"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some(desc))
                .size(12)
                .text_color("#cccccc")
                .padding(12)
                .bg("#141414")
                .radius(10),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .width_full()
                .margin_top(12)
                .gap(8)
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("取消"))
                        .bg("#1e1e1e")
                        .text_color("#FF4444")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CANCEL_SAVE),
                )
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("确认推送"))
                        .bg("#00AA66")
                        .text_color("#ffffff")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CONFIRM_SAVE),
                ),
        );

    dialog_overlay(content)
}

fn build_delete_confirm_dialog(state: &UiState) -> ui::Element {
    let name = state
        .delete_confirm_index
        .and_then(|idx| sync::read_state(|s| s.local_entries.get(idx).map(|e| e.name.clone())))
        .unwrap_or_else(|| "此条主机".to_string());
    let delete_desc = format!(
        "确定要删除「{}」吗？删除后本地立即移除，需推送到手表才正式生效。",
        name
    );

    let content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("确认删除？"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(6),
        )
        .child(
            ui::Element::new(
                ui::ElementType::P,
                Some(&delete_desc),
            )
            .size(12)
            .text_color("#cccccc")
            .margin_bottom(16),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .width_full()
                .gap(8)
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("取消"))
                        .bg("#1e1e1e")
                        .text_color("#ffffff")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CANCEL_DELETE_ENTRY),
                )
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("确认删除"))
                        .bg("#FF4444")
                        .text_color("#ffffff")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CONFIRM_DELETE_ENTRY),
                ),
        );

    dialog_overlay(content)
}

fn build_edit_dialog(state: &UiState) -> ui::Element {
    let entry = &state.edit_form_data;
    let delete_event = if let Some(idx) = state.edit_form_index {
        format!("{}{}", EVENT_DELETE_ENTRY, idx)
    } else {
        EVENT_DELETE_ENTRY.to_string()
    };
    let ev_none = "edit_encryptMode_0".to_string();
    let ev_sign = "edit_encryptMode_1".to_string();
    let ev_sm4 = "edit_encryptMode_3".to_string();

    let mut content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("编辑主机"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(form_input("名称", EDIT_NAME, &entry.name))
        .child(form_input("URL / 服务地址", EDIT_URL, &entry.url))
        .child(encrypt_mode_select(
            entry.encrypt_mode,
            &ev_none,
            &ev_sign,
            &ev_sm4,
        ));

    if entry.encrypt_mode == 1 {
        content = content.child(form_input(
            "签名密钥 (secret)",
            EDIT_SECRET,
            &entry.secret,
        ));
    }
    if entry.encrypt_mode == 3 {
        content = content.child(form_input(
            "SM4 密钥 (32 hex)",
            EDIT_SM4,
            entry.sm4_key_hex.as_deref().unwrap_or(""),
        ));
    }

    content = content
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .width_full()
                .margin_top(12)
                .gap(8)
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("取消"))
                        .bg("#1e1e1e")
                        .text_color("#FF4444")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CANCEL_EDIT),
                )
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("保存"))
                        .bg("#00AA66")
                        .text_color("#ffffff")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_SAVE_EDIT),
                ),
        )
        .child(
            ui::Element::new(ui::ElementType::Button, Some("删除此主机"))
                .bg("#2a2a2a")
                .text_color("#FF4444")
                .size(13)
                .padding(8)
                .radius(8)
                .width_full()
                .margin_top(8)
                .on(ui::Event::Click, &delete_event),
        );

    dialog_overlay(content)
}

fn build_add_dialog(state: &UiState) -> ui::Element {
    let entry = &state.add_form_data;
    let ev_none = "add_encryptMode_0".to_string();
    let ev_sign = "add_encryptMode_1".to_string();
    let ev_sm4 = "add_encryptMode_3".to_string();

    let mut content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("添加主机"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(form_input("名称", ADD_NAME, &entry.name))
        .child(form_input("URL / 服务地址", ADD_URL, &entry.url))
        .child(encrypt_mode_select(
            entry.encrypt_mode,
            &ev_none,
            &ev_sign,
            &ev_sm4,
        ));

    if entry.encrypt_mode == 1 {
        content = content.child(form_input(
            "签名密钥 (secret)",
            ADD_SECRET,
            &entry.secret,
        ));
    }
    if entry.encrypt_mode == 3 {
        content = content.child(form_input(
            "SM4 密钥 (32 hex)",
            ADD_SM4,
            entry.sm4_key_hex.as_deref().unwrap_or(""),
        ));
    }

    content = content.child(
        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .width_full()
            .margin_top(12)
            .gap(8)
            .child(
                ui::Element::new(ui::ElementType::Button, Some("取消"))
                    .bg("#1e1e1e")
                    .text_color("#FF4444")
                    .size(14)
                    .padding(10)
                    .radius(10)
                    .flex_grow(1.0)
                    .on(ui::Event::Click, EVENT_CANCEL_ADD),
            )
            .child(
                ui::Element::new(ui::ElementType::Button, Some("保存"))
                    .bg("#00AA66")
                    .text_color("#ffffff")
                    .size(14)
                    .padding(10)
                    .radius(10)
                    .flex_grow(1.0)
                    .on(ui::Event::Click, EVENT_SAVE_ADD),
            ),
    );

    dialog_overlay(content)
}

fn form_input(label: &str, id: &str, value: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .margin_bottom(8)
        .child(
            ui::Element::new(ui::ElementType::P, Some(label))
                .size(12)
                .text_color("#a0a0a0"),
        )
        .child(
            ui::Element::new(ui::ElementType::Input, Some(value))
                .width_full()
                .padding(8)
                .bg("#1e1e1e")
                .radius(8)
                .on(ui::Event::Input, id)
                .on(ui::Event::Change, id)
                .on(ui::Event::Blur, id),
        )
}



fn select_btn(label: &str, event_id: &str, is_selected: bool) -> ui::Element {
    let bg = if is_selected { "#0090FF" } else { "#1e1e1e" };
    let text_color = if is_selected { "#ffffff" } else { "#a0a0a0" };
    ui::Element::new(ui::ElementType::Button, Some(label))
        .bg(bg)
        .text_color(text_color)
        .size(13)
        .padding(6)
        .radius(6)
        .flex_grow(1.0)
        .on(ui::Event::Click, event_id)
}

fn encrypt_mode_select(
    selected: u32,
    ev_none: &str,
    ev_sign: &str,
    ev_sm4: &str,
) -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .margin_bottom(8)
        .child(
            ui::Element::new(ui::ElementType::P, Some("加密模式"))
                .size(12)
                .text_color("#a0a0a0"),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .gap(6)
                .child(select_btn("无加密", ev_none, selected == 0))
                .child(select_btn("签名HMAC", ev_sign, selected == 1))
                .child(select_btn("SM4", ev_sm4, selected == 3)),
        )
}

fn dialog_overlay(content: ui::Element) -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .flex()
        .justify_center()
        .bg("#00000080")
        .child(content)
}
