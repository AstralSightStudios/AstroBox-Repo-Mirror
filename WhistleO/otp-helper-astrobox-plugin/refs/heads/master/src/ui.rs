//! UI 模块：单页滚动式声明式界面
//!
//! 布局从上到下依次为：
//! - 连接设置：打开应用
//! - 快速添加：批量快速添加
//! - 手机管理设备数据：加载至手机、修改同步至设备、批量导出
//! - X个账号 标题
//! - 账号列表区域

use crate::astrobox::psys_host::ui as ui_old;
use crate::astrobox::psys_host::ui_v3 as ui;
use std::sync::{Mutex, OnceLock};

use crate::otp;
use crate::sync::{self, TotpEntry};

// ─── 事件 ID 常量 ───────────────────────────────────────────────

// 快速添加区
pub const EVENT_QUICK_ADD_SUBMIT: &str = "quick_add_submit";
pub const ID_URI_INPUT: &str = "uri_input";

// 数据管理区
pub const EVENT_LOAD_FROM_WATCH: &str = "load_from_watch";
pub const EVENT_SAVE_TO_WATCH: &str = "save_to_watch";
pub const EVENT_EXPORT_ALL: &str = "export_all";
pub const EVENT_CLOSE_EXPORT: &str = "close_export";
pub const EVENT_OPEN_QUICK_ADD: &str = "open_quick_add";
pub const EVENT_CLOSE_QUICK_ADD: &str = "close_quick_add";
pub const EVENT_ADD_ACCOUNT: &str = "add_account";
pub const EVENT_REFRESH_CODES: &str = "refresh_codes";

// Tab 切换
pub const EVENT_TAB_ACCOUNTS: &str = "tab_accounts";
pub const EVENT_TAB_TOOLS: &str = "tab_tools";

// 保存确认
pub const EVENT_CONFIRM_SAVE: &str = "confirm_save";
pub const EVENT_CANCEL_SAVE: &str = "cancel_save";

// 账号列表
pub const EVENT_DELETE_ENTRY: &str = "delete_entry_";
pub const EVENT_EDIT_ENTRY: &str = "edit_entry_";

// 编辑表单
pub const EVENT_SAVE_EDIT: &str = "save_edit";
pub const EVENT_CANCEL_EDIT: &str = "cancel_edit";
pub const EVENT_SAVE_ADD: &str = "save_add";
pub const EVENT_CANCEL_ADD: &str = "cancel_add";

// 表单字段
pub const EDIT_NAME: &str = "edit_name";
pub const EDIT_ISSUER: &str = "edit_issuer";
pub const EDIT_SECRET: &str = "edit_secret";
pub const EDIT_TYPE: &str = "edit_type";
pub const EDIT_ALGO: &str = "edit_algo";
pub const EDIT_DIGITS: &str = "edit_digits";
pub const EDIT_PERIOD: &str = "edit_period";

pub const EVENT_CANCEL_DELETE_ENTRY: &str = "cancel_delete_entry";
pub const EVENT_CONFIRM_DELETE_ENTRY: &str = "confirm_delete_entry";

// 状态文本
pub const ID_STATUS_TEXT: &str = "status_text";

/// 创建默认的 TotpEntry（用于表单初始化）
fn default_totp_entry() -> sync::TotpEntry {
    sync::TotpEntry {
        id: String::new(),
        name: String::new(),
        issuer: None,
        secret: String::new(),
        otp_type: "totp".to_string(),
        algorithm: "SHA1".to_string(),
        digits: 6,
        period: 30,
        counter: None,
        extra_params: None,
        created_at: None,
    }
}

// ─── Tab 枚举 ───────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Accounts,
    Tools,
}

// ─── UI 状态 ────────────────────────────────────────────────────

struct UiState {
    root_element_id: Option<String>,
    last_status: String,
    uri_text: String,
    /// 是否正在显示保存确认对话框
    show_save_confirm: bool,
    /// 保存确认时显示的修改项描述
    save_confirm_desc: String,
    /// 导出的 URI 文本
    export_text: String,
    /// 是否正在显示导出区
    show_export: bool,
    /// 是否显示编辑表单
    show_edit_form: bool,
    /// 正在编辑的账号索引
    edit_form_index: Option<usize>,
    /// 编辑表单数据
    edit_form_data: sync::TotpEntry,
    /// 是否显示添加表单
    show_add_form: bool,
    /// 添加表单数据
    add_form_data: sync::TotpEntry,
    /// 当前激活的 Tab
    active_tab: Tab,
    /// 是否显示删除确认对话框
    show_delete_confirm: bool,
    /// 待删除的账号索引
    delete_confirm_index: Option<usize>,
    /// 是否显示批量快速添加弹窗
    show_quick_add_dialog: bool,
}

static UI_STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

fn ui_state() -> &'static Mutex<UiState> {
    UI_STATE.get_or_init(|| {
        Mutex::new(UiState {
            root_element_id: None,
            last_status: "准备就绪".to_string(),
            uri_text: String::new(),
            show_save_confirm: false,
            save_confirm_desc: String::new(),
            export_text: String::new(),
            show_export: false,
            show_edit_form: false,
            edit_form_index: None,
            edit_form_data: default_totp_entry(),
            show_add_form: false,
            add_form_data: default_totp_entry(),
            active_tab: Tab::Accounts,
            show_delete_confirm: false,
            delete_confirm_index: None,
            show_quick_add_dialog: false,
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

/// 检查当前是否有弹窗打开（用于定时器暂停刷新）
pub fn is_dialog_open() -> bool {
    let state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.show_edit_form || state.show_add_form || state.show_save_confirm || state.show_export || state.show_delete_confirm || state.show_quick_add_dialog
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

/// 直接更新状态文本（不自动刷新 UI，由调用方负责刷新）
pub fn update_status_direct(message: &str) {
    let mut state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_status = message.to_string();
}

// ─── 事件处理 ────────────────────────────────────────────────────

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
            EVENT_QUICK_ADD_SUBMIT => handle_quick_add(),
            EVENT_LOAD_FROM_WATCH => handle_load_from_watch(),
            EVENT_SAVE_TO_WATCH => handle_save_to_watch(),
            EVENT_CONFIRM_SAVE => handle_confirm_save(),
            EVENT_CANCEL_SAVE => handle_cancel_save(),
            EVENT_EXPORT_ALL => handle_export_all(),
            EVENT_CLOSE_EXPORT => handle_close_export(),
            EVENT_OPEN_QUICK_ADD => handle_open_quick_add(),
            EVENT_CLOSE_QUICK_ADD => handle_close_quick_add(),
            EVENT_ADD_ACCOUNT => handle_add_account(),
            EVENT_REFRESH_CODES => refresh_ui(),
            EVENT_SAVE_EDIT => handle_save_edit(),
            EVENT_CANCEL_EDIT => handle_cancel_edit(),
            EVENT_SAVE_ADD => handle_save_add(),
            EVENT_CANCEL_ADD => handle_cancel_add(),
            EVENT_TAB_ACCOUNTS => handle_tab_switch(Tab::Accounts),
            EVENT_TAB_TOOLS => handle_tab_switch(Tab::Tools),
            EVENT_CONFIRM_DELETE_ENTRY => handle_confirm_delete_entry(),
            EVENT_CANCEL_DELETE_ENTRY => handle_cancel_delete_entry(),
            _ if event_id.starts_with(EVENT_DELETE_ENTRY) => {
                if let Ok(idx) = event_id.trim_start_matches(EVENT_DELETE_ENTRY).parse::<usize>() {
                    handle_delete_entry(idx);
                }
            }
            _ if event_id.starts_with(EVENT_EDIT_ENTRY) => {
                if let Ok(idx) = event_id.trim_start_matches(EVENT_EDIT_ENTRY).parse::<usize>() {
                    handle_edit_entry(idx);
                }
            }
            _ if event_id.starts_with("edit_type_")
                || event_id.starts_with("add_type_")
                || event_id.starts_with("edit_algo_")
                || event_id.starts_with("add_algo_")
                || event_id.starts_with("edit_digits_")
                || event_id.starts_with("add_digits_")
                || event_id.starts_with("edit_period_")
                || event_id.starts_with("add_period_")
                || event_id.starts_with("edit_counter_")
                || event_id.starts_with("add_counter_") =>
            {
                handle_form_select(event_id);
            }
            _ => {}
        },
        ui::Event::Change => {
            if event_id == ID_URI_INPUT {
                if let Some(value) = parse_input_value(payload, event_id) {
                    let mut state = ui_state()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.uri_text = value;
                }
            } else if event_id.starts_with("edit_") || event_id.starts_with("add_") {
                update_form_field(event_id, payload);
            }
        }
        ui::Event::Input | ui::Event::Blur => {
            if event_id == ID_URI_INPUT {
                if let Some(value) = parse_input_value(payload, event_id) {
                    let mut state = ui_state()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.uri_text = value;
                }
            } else if event_id.starts_with("edit_") || event_id.starts_with("add_") {
                update_form_field(event_id, payload);
            }
        }
        _ => {
            tracing::warn!("unhandled event: evtype={:?}, event_id={}", evtype, event_id);
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



// ─── 快速添加处理 ────────────────────────────────────────────────

fn handle_open_quick_add() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_quick_add_dialog = true;
    }
    refresh_ui();
}

fn handle_close_quick_add() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_quick_add_dialog = false;
    }
    refresh_ui();
}

fn handle_quick_add() {
    let text = {
        let state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.uri_text.clone()
    };

    update_status("正在解析并添加...");

    let entries = sync::parse_otpauth_text(&text);
    if entries.is_empty() {
        update_status("没有解析到有效的 otpauth:// URI");
        return;
    }

    // 非阻塞：使用 spawn 异步执行
    wit_bindgen::spawn(async move {
        match sync::add_accounts_to_watch(&entries).await {
            Ok(msg) => {
                // 同时添加到本地列表
                for entry in &entries {
                    sync::add_local_entry(entry.clone());
                }
                update_status(&format!("快速添加成功：{}", msg));
                // 清空输入框
                {
                    let mut state = ui_state()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.uri_text = String::new();
                }
                refresh_ui();
            }
            Err(e) => update_status(&format!("快速添加失败：{}", e)),
        }
    });
}

// ─── Tab 切换处理 ────────────────────────────────────────────────

fn handle_tab_switch(tab: Tab) {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active_tab = tab;
    }
    refresh_ui();
}

// ─── 数据管理处理 ────────────────────────────────────────────────

fn handle_load_from_watch() {
    if sync::is_loading() {
        update_status("正在从手表加载数据中，请稍候...");
        return;
    }

    update_status("正在连接设备...");
    wit_bindgen::spawn(async move {
        // 1. 检查设备连接
        let device_addr = match crate::device::check_device().await {
            Some(addr) => addr,
            None => {
                update_status("未检测到已连接设备");
                return;
            }
        };

        // 2. 解析目标应用包名
        let pkg_name = match crate::device::resolve_pkg_name(&device_addr).await {
            Some(name) => name,
            None => {
                update_status("未找到认证器应用");
                return;
            }
        };

        // 3. 启动手表应用并等待就绪
        update_status("正在打开手表应用...");
        if let Err(e) = crate::device::launch_and_wait(&device_addr, &pkg_name).await {
            update_status(&format!("打开应用失败：{}", e));
            return;
        }

        // 4. 请求账号列表
        update_status("应用已就绪，请求账号列表...");
        if let Err(e) = sync::request_list_accounts().await {
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

    // 二次确认：显示增量描述
    let mut desc_lines: Vec<String> = Vec::new();
    if !upsert.is_empty() {
        desc_lines.push(format!("新增/修改 {} 项：", upsert.len()));
        for e in &upsert {
            let issuer = e.issuer.as_deref().unwrap_or("");
            if issuer.is_empty() {
                desc_lines.push(format!("  {} ({}位 {})", e.name, e.digits, e.otp_type.to_uppercase()));
            } else {
                desc_lines.push(format!("  {} - {} ({}位 {})", issuer, e.name, e.digits, e.otp_type.to_uppercase()));
            }
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

    // 非阻塞：使用 spawn 异步执行
    wit_bindgen::spawn(async move {
        match sync::send_sync_delta().await {
            Ok(msg) => {
                update_status(&format!("保存成功：{}", msg));
            }
            Err(e) => update_status(&format!("保存失败：{}", e)),
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
        state.show_edit_form = false;
        state.edit_form_index = None;
        state.edit_form_data = default_totp_entry();
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

fn handle_export_all() {
    let uris = sync::export_all_as_uris();
    if uris.is_empty() {
        update_status("暂无数据可导出，请先从手表加载");
        return;
    }
    let text = uris.join("\n");
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.export_text = text;
        state.show_export = true;
    }
    update_status(&format!("已生成 {} 条 otpauth:// 链接", uris.len()));
}

fn handle_close_export() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_export = false;
    }
    refresh_ui();
}

// ─── 账号操作处理 ────────────────────────────────────────────────

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
            sync::update_local_entry_by_id(&old.id, entry);
            update_status("已保存编辑（需保存修改至手表生效）");
        }
    }
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_edit_form = false;
        state.edit_form_index = None;
        state.edit_form_data = default_totp_entry();
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
        state.edit_form_data = default_totp_entry();
    }
    refresh_ui();
}

fn handle_add_account() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_add_form = true;
        state.add_form_data = default_totp_entry();
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
            e.id = format!("acc_{}_{}", now, rand_simple());
            e.created_at = Some(now);
        }
        e
    };
    sync::add_local_entry(entry);
    update_status("已添加账号（需保存修改至手表生效）");
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_add_form = false;
        state.add_form_data = default_totp_entry();
    }
    refresh_ui();
}

fn handle_cancel_add() {
    {
        let mut state = ui_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.show_add_form = false;
        state.add_form_data = default_totp_entry();
    }
    refresh_ui();
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
            // 将 add_xxx 映射为 edit_xxx 以便统一匹配
            &event_id.replacen("add_", "edit_", 1)
        };
        match field_name {
            EDIT_NAME => entry.name = value,
            EDIT_ISSUER => entry.issuer = if value.is_empty() { None } else { Some(value) },
            EDIT_SECRET => entry.secret = value,
            EDIT_TYPE => entry.otp_type = value,
            EDIT_ALGO => entry.algorithm = value,
            EDIT_DIGITS => {
                if let Ok(d) = value.parse::<u32>() {
                    entry.digits = d;
                }
            }
            "edit_counter" => {
                if let Ok(c) = value.parse::<u64>() {
                    entry.counter = Some(c);
                }
            }
            "edit_period" => {
                if let Ok(p) = value.parse::<u32>() {
                    entry.period = p;
                }
            }
            _ => {}
        }
    }
}

fn handle_form_select(event_id: &str) {
    let mut state = ui_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let is_edit = event_id.starts_with("edit_");
    let entry = if is_edit {
        &mut state.edit_form_data
    } else {
        &mut state.add_form_data
    };

    match event_id {
        "edit_type_totp" | "add_type_totp" => entry.otp_type = "totp".to_string(),
        "edit_type_hotp" | "add_type_hotp" => entry.otp_type = "hotp".to_string(),
        "edit_algo_sha1" | "add_algo_sha1" => entry.algorithm = "SHA1".to_string(),
        "edit_algo_sha256" | "add_algo_sha256" => entry.algorithm = "SHA256".to_string(),
        "edit_algo_sha512" | "add_algo_sha512" => entry.algorithm = "SHA512".to_string(),
        "edit_digits_6" | "add_digits_6" => entry.digits = 6,
        "edit_digits_8" | "add_digits_8" => entry.digits = 8,
        "edit_period_30" | "add_period_30" => entry.period = 30,
        "edit_period_custom" | "add_period_custom" => {
            if entry.period == 30 {
                entry.period = 60;
            }
        }
        "edit_counter_0" | "add_counter_0" => entry.counter = Some(0),
        "edit_counter_custom" | "add_counter_custom" => {
            if entry.counter == Some(0) || entry.counter.is_none() {
                entry.counter = Some(1);
            }
        }
        _ => {}
    }
    drop(state);
    refresh_ui();
}

fn rand_simple() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ─── interconnect 结果处理 ───────────────────────────────────────

pub fn apply_interconnect_result(result: sync::InterconnectResult) {
    match result {
        sync::InterconnectResult::AccountList(entries) => {
            update_status(&format!("已加载 {} 条账号", entries.len()));
        }
        sync::InterconnectResult::OperationResult { message, is_error } => {
            update_status(&message);
            if !is_error {
                tracing::info!("operation success: {}", message);
            }
        }
    }
}

// ─── UI 构建 ─────────────────────────────────────────────────────

fn build_ui(state: &UiState) -> ui::Element {
    // 状态栏
    let status = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .padding(10)
        .bg("#1a1a1a")
        .radius(10)
        .margin_bottom(10)
        .child(ui::Element::new(ui::ElementType::P, Some(&state.last_status))
            .size(12)
            .text_color("#a0a0a0"));

    // Tab 切换按钮
    let tab_trigger = |label: &str, tab: Tab, event: &str| -> ui::Element {
        let is_active = state.active_tab == tab;
        let bg = if is_active { "#0090FF" } else { "#1e1e1e" };
        let text = if is_active { "#ffffff" } else { "#a0a0a0" };
        ui::Element::new(ui::ElementType::Button, Some(label))
            .bg(bg)
            .text_color(text)
            .size(14)
            .padding(8)
            .radius(10)
            .flex_grow(1.0)
            .on(ui::Event::Click, event)
    };

    let tabs_list = ui::Element::new(ui::ElementType::TabsList, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .width_full()
        .gap(8)
        .margin_bottom(10)
        .child(tab_trigger("账号列表", Tab::Accounts, EVENT_TAB_ACCOUNTS))
        .child(tab_trigger("批量工具", Tab::Tools, EVENT_TAB_TOOLS));

    let tabs_content = match state.active_tab {
        Tab::Accounts => build_accounts_tab(state),
        Tab::Tools => build_tools_tab(state),
    };

    let tabs_root = ui::Element::new(ui::ElementType::TabsRoot, None)
        .width_full()
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .child(tabs_list)
        .child(
            ui::Element::new(ui::ElementType::TabsContent, None)
                .width_full()
                .child(tabs_content),
        );

    let any_dialog = state.show_edit_form || state.show_add_form || state.show_save_confirm || state.show_export || state.show_delete_confirm || state.show_quick_add_dialog;

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
        } else if state.show_export {
            root = root.child(build_export_dialog(state));
        } else if state.show_quick_add_dialog {
            root = root.child(build_quick_add_dialog(state));
        }
    } else {
        root = root.child(tabs_root);
    }

    root
}



fn mask_code(digits: u32) -> String {
    let count = digits as usize;
    let mut s = String::new();
    for _ in 0..count {
        s.push('-');
    }
    if count > 4 {
        let mid = (count + 1) / 2;
        format!("{} {}", &s[..mid], &s[mid..])
    } else {
        s
    }
}

fn resolve_account_row(
    entry: &TotpEntry,
    now_secs: u64,
) -> (String, String, String, String, String) {
    let name = entry.name.clone();

    if entry.otp_type == "totp" {
        match otp::generate_totp(&entry.secret, entry.digits, entry.period, &entry.algorithm, now_secs) {
            Ok((code, remaining)) => (
                name,
                code,
                format!("{}秒", remaining),
                "#7de7a3".to_string(),
                "#1d1d1f".to_string(),
            ),
            Err(_) => (
                name,
                mask_code(entry.digits),
                "异常".to_string(),
                "#ff9aa5".to_string(),
                "rgba(255,94,109,0.16)".to_string(),
            ),
        }
    } else if entry.otp_type == "hotp" {
        let counter = entry.counter.unwrap_or(0);
        match otp::generate_hotp(&entry.secret, entry.digits, &entry.algorithm, counter) {
            Ok(code) => (
                name,
                code,
                format!("#{}", counter),
                "#7de7a3".to_string(),
                "#1d1d1f".to_string(),
            ),
            Err(_) => (
                name,
                mask_code(entry.digits),
                "异常".to_string(),
                "#ff9aa5".to_string(),
                "rgba(255,94,109,0.16)".to_string(),
            ),
        }
    } else {
        (
            name,
            mask_code(entry.digits),
            "兼容".to_string(),
            "#d2d2d7".to_string(),
            "rgba(255,255,255,0.08)".to_string(),
        )
    }
}

// ─── Tab 1: 账号列表 ─────────────────────────────────────────────

fn build_accounts_tab(_state: &UiState) -> ui::Element {
    let entries = sync::read_state(|s| s.local_entries.clone());
    let (upsert, remove) = sync::get_sync_delta();
    let pending_count = upsert.len() + remove.len();

    let save_label = if pending_count > 0 {
        format!("保存修改 ({})", pending_count)
    } else {
        "保存修改".to_string()
    };
    let save_bg = if pending_count > 0 { "#00AA66" } else { "#1e1e1e" };
    let save_text = if pending_count > 0 { "#ffffff" } else { "#666666" };

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
                .on(ui::Event::Click, EVENT_ADD_ACCOUNT),
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
                    ui::Element::new(ui::ElementType::P, Some("暂无账号"))
                        .text_color("#888888")
                        .size(14)
                        .margin_bottom(6),
                )
                .child(
                    ui::Element::new(ui::ElementType::P, Some("点击上方按钮新建或拉取手表数据"))
                        .text_color("#666666")
                        .size(12),
                ),
        );
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for (i, entry) in entries.iter().enumerate() {
            let edit_event = format!("{}{}", EVENT_EDIT_ENTRY, i);
            let (name, code, side_label, side_color, side_bg) = resolve_account_row(entry, now);

            let row = ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .padding(12)
                .bg("#141414")
                .radius(10)
                .margin_bottom(8)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .align_center()
                .on(ui::Event::Click, &edit_event)
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .flex()
                        .flex_direction(ui::FlexDirection::Column)
                        .flex_grow(1.0)
                        .child(
                            ui::Element::new(ui::ElementType::P, Some(&name))
                                .size(14)
                                .text_color("#ffffff"),
                        )
                        .child(
                            ui::Element::new(ui::ElementType::P, Some(&code))
                                .size(16)
                                .text_color("#ffffff")
                                .margin_top(2),
                        ),
                )
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .padding(6)
                        .radius(6)
                        .bg(&side_bg)
                        .child(
                            ui::Element::new(ui::ElementType::P, Some(&side_label))
                                .size(12)
                                .text_color(&side_color),
                        ),
                );

            list = list.child(row);
        }
    }

    let scroll_area = ui::Element::new(ui::ElementType::ScrollArea, None)
        .width_full()
        .child(list);

    let refresh_hint = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .margin_bottom(8)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .gap(8)
        .align_center()
        .child(
            ui::Element::new(ui::ElementType::Button, Some("刷新动态口令"))
                .bg("#1e1e1e")
                .text_color("#ffffff")
                .size(12)
                .padding(6)
                .radius(6)
                .on(ui::Event::Click, EVENT_REFRESH_CODES),
        );

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(action_bar)
        .child(refresh_hint)
        .child(scroll_area)
}

// ─── Tab 2: 批量工具 ─────────────────────────────────────────────

fn build_tools_tab(_state: &UiState) -> ui::Element {
    let quick_add = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .padding(14)
        .bg("#1a1a1a")
        .radius(12)
        .margin_bottom(10)
        .border(1, "#2a2a2a")
        .child(
            ui::Element::new(ui::ElementType::P, Some("批量快速添加"))
                .size(16)
                .text_color("#ffffff")
                .margin_bottom(6),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some("每行粘贴一个 otpauth:// URI，系统将自动解析并添加"))
                .size(12)
                .text_color("#888888")
                .margin_bottom(8),
        )
        .child(
            ui::Element::new(ui::ElementType::Button, Some("打开批量添加"))
                .bg("#00AA66")
                .text_color("#ffffff")
                .size(14)
                .padding(10)
                .radius(10)
                .width_full()
                .on(ui::Event::Click, EVENT_OPEN_QUICK_ADD),
        );

    let export_section = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .padding(14)
        .bg("#1a1a1a")
        .radius(12)
        .border(1, "#2a2a2a")
        .child(
            ui::Element::new(ui::ElementType::P, Some("批量导出"))
                .size(16)
                .text_color("#ffffff")
                .margin_bottom(6),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some("将所有账号导出为 otpauth:// URI 格式"))
                .size(12)
                .text_color("#888888")
                .margin_bottom(8),
        )
        .child(
            ui::Element::new(ui::ElementType::Button, Some("生成并查看导出内容"))
                .bg("#1e1e1e")
                .text_color("#ffffff")
                .size(14)
                .padding(10)
                .radius(10)
                .width_full()
                .on(ui::Event::Click, EVENT_EXPORT_ALL),
        );

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(quick_add)
        .child(export_section)
}

// ─── 弹窗辅助函数 ────────────────────────────────────────────────

fn form_input(label: &str, id: &str, value: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .margin_bottom(8)
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(12).text_color("#a0a0a0"))
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

// ─── 弹窗：编辑账号 ──────────────────────────────────────────────

fn build_edit_dialog(state: &UiState) -> ui::Element {
    let entry = &state.edit_form_data;
    let issuer_val = entry.issuer.as_deref().unwrap_or("");
    let delete_event = if let Some(idx) = state.edit_form_index {
        format!("{}{}", EVENT_DELETE_ENTRY, idx)
    } else {
        EVENT_DELETE_ENTRY.to_string()
    };

    let mut content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("编辑账号"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(form_input("名称", EDIT_NAME, &entry.name))
        .child(form_input("颁发者（可选）", EDIT_ISSUER, issuer_val))
        .child(form_input("密钥", EDIT_SECRET, &entry.secret))
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("类型")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("TOTP", "edit_type_totp", entry.otp_type == "totp"))
                        .child(select_btn("HOTP", "edit_type_hotp", entry.otp_type == "hotp")),
                ),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("算法")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("SHA1", "edit_algo_sha1", entry.algorithm == "SHA1"))
                        .child(select_btn("SHA256", "edit_algo_sha256", entry.algorithm == "SHA256"))
                        .child(select_btn("SHA512", "edit_algo_sha512", entry.algorithm == "SHA512")),
                ),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("位数")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("6", "edit_digits_6", entry.digits == 6))
                        .child(select_btn("8", "edit_digits_8", entry.digits == 8)),
                ),
        );

    if entry.otp_type == "totp" {
        let is_custom_period = entry.period != 30;
        content = content.child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("周期")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("30s", "edit_period_30", entry.period == 30))
                        .child(select_btn("自定义", "edit_period_custom", is_custom_period)),
                ),
        );
        if is_custom_period {
            content = content.child(form_input(
                "周期值（秒）",
                "edit_period",
                &entry.period.to_string(),
            ));
        }
    } else {
        let is_custom_counter = entry.counter != Some(0);
        content = content.child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("计数器")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("0", "edit_counter_0", entry.counter == Some(0)))
                        .child(select_btn("自定义", "edit_counter_custom", is_custom_counter)),
                ),
        );
        if is_custom_counter {
            content = content.child(form_input(
                "计数器值",
                "edit_counter",
                &entry.counter.map(|c| c.to_string()).unwrap_or_default(),
            ));
        }
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
            ui::Element::new(ui::ElementType::Button, Some("删除此账号"))
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

// ─── 弹窗：添加账号 ──────────────────────────────────────────────

fn build_add_dialog(state: &UiState) -> ui::Element {
    let entry = &state.add_form_data;
    let issuer_val = entry.issuer.as_deref().unwrap_or("");

    let mut content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("添加账号"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(form_input("名称", "add_name", &entry.name))
        .child(form_input("颁发者（可选）", "add_issuer", issuer_val))
        .child(form_input("密钥", "add_secret", &entry.secret))
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("类型")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("TOTP", "add_type_totp", entry.otp_type == "totp"))
                        .child(select_btn("HOTP", "add_type_hotp", entry.otp_type == "hotp")),
                ),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("算法")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("SHA1", "add_algo_sha1", entry.algorithm == "SHA1"))
                        .child(select_btn("SHA256", "add_algo_sha256", entry.algorithm == "SHA256"))
                        .child(select_btn("SHA512", "add_algo_sha512", entry.algorithm == "SHA512")),
                ),
        )
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("位数")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("6", "add_digits_6", entry.digits == 6))
                        .child(select_btn("8", "add_digits_8", entry.digits == 8)),
                ),
        );

    if entry.otp_type == "totp" {
        let is_custom_period = entry.period != 30;
        content = content.child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("周期")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("30s", "add_period_30", entry.period == 30))
                        .child(select_btn("自定义", "add_period_custom", is_custom_period)),
                ),
        );
        if is_custom_period {
            content = content.child(form_input(
                "周期值（秒）",
                "add_period",
                &entry.period.to_string(),
            ));
        }
    } else {
        let is_custom_counter = entry.counter != Some(0);
        content = content.child(
            ui::Element::new(ui::ElementType::Div, None)
                .width_full()
                .margin_bottom(8)
                .child(ui::Element::new(ui::ElementType::P, Some("计数器")).size(12).text_color("#a0a0a0"))
                .child(
                    ui::Element::new(ui::ElementType::Div, None)
                        .width_full()
                        .flex()
                        .flex_direction(ui::FlexDirection::Row)
                        .gap(6)
                        .child(select_btn("0", "add_counter_0", entry.counter == Some(0)))
                        .child(select_btn("自定义", "add_counter_custom", is_custom_counter)),
                ),
        );
        if is_custom_counter {
            content = content.child(form_input(
                "计数器值",
                "add_counter",
                &entry.counter.map(|c| c.to_string()).unwrap_or_default(),
            ));
        }
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

// ─── 弹窗：保存确认 ──────────────────────────────────────────────

fn build_save_confirm_dialog(desc: &str) -> ui::Element {
    let content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("确认同步修改"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some(desc))
                .size(13)
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
                        .text_color("#FF4444")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CANCEL_SAVE),
                )
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("确认同步"))
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

// ─── 弹窗：删除确认 ──────────────────────────────────────────────

fn build_delete_confirm_dialog(state: &UiState) -> ui::Element {
    let name = state
        .delete_confirm_index
        .and_then(|idx| sync::read_state(|s| s.local_entries.get(idx).map(|e| e.name.clone())))
        .unwrap_or_else(|| "此账号".to_string());

    let content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("确认删除账号"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some(&format!("确定要删除「{}」吗？删除后需保存修改至手表生效。", name)))
                .size(13)
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

// ─── 弹窗：导出结果 ──────────────────────────────────────────────

fn build_quick_add_dialog(state: &UiState) -> ui::Element {
    let content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("批量快速添加"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some("每行粘贴一个 otpauth:// URI，系统将自动解析并添加"))
                .size(12)
                .text_color("#888888")
                .margin_bottom(8),
        )
        .child(
            ui::Element::new(ui::ElementType::Textarea, Some(&state.uri_text))
                .width_full()
                .height(160)
                .margin_bottom(12)
                .on(ui::Event::Input, ID_URI_INPUT)
                .on(ui::Event::Change, ID_URI_INPUT)
                .on(ui::Event::Blur, ID_URI_INPUT),
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
                        .text_color("#FF4444")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_CLOSE_QUICK_ADD),
                )
                .child(
                    ui::Element::new(ui::ElementType::Button, Some("添加到手表"))
                        .bg("#00AA66")
                        .text_color("#ffffff")
                        .size(14)
                        .padding(10)
                        .radius(10)
                        .flex_grow(1.0)
                        .on(ui::Event::Click, EVENT_QUICK_ADD_SUBMIT),
                ),
        );

    dialog_overlay(content)
}

fn build_export_dialog(state: &UiState) -> ui::Element {
    let content = ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .max_width(420)
        .padding(16)
        .bg("#242424")
        .radius(16)
        .child(
            ui::Element::new(ui::ElementType::P, Some("批量导出"))
                .size(18)
                .text_color("#ffffff")
                .margin_bottom(12),
        )
        .child(
            ui::Element::new(ui::ElementType::P, Some("以下是导出的 otpauth:// URI 列表："))
                .size(12)
                .text_color("#888888")
                .margin_bottom(8),
        )
        .child(
            ui::Element::new(ui::ElementType::Textarea, Some(&state.export_text))
                .width_full()
                .height(200)
                .margin_bottom(12)
                .bg("#1e1e1e")
                .radius(8),
        )
        .child(
            ui::Element::new(ui::ElementType::Button, Some("关闭"))
                .bg("#1e1e1e")
                .text_color("#ffffff")
                .size(14)
                .padding(10)
                .radius(10)
                .width_full()
                .on(ui::Event::Click, EVENT_CLOSE_EXPORT),
        );

    dialog_overlay(content)
}

// ─── 弹窗遮罩层 ──────────────────────────────────────────────────

fn dialog_overlay(content: ui::Element) -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .width_full()
        .flex()
        .justify_center()
        .child(content)
}
