use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui_v3 as ui;
use crate::ui::icons;
use crate::ui::state::{self, AppConnectionStatus, InstalledApp, MainTab};

// ========== 事件ID常量 ==========

pub const TAB_CONNECT_EVENT: &str = "tab_connect";
pub const TAB_ABOUT_EVENT: &str = "tab_about";
pub const EVENT_REFRESH_DEVICES: &str = "action:devices.refresh";
pub const APP_CONNECT_PREFIX: &str = "app:connect:";
pub const APP_DISCONNECT_PREFIX: &str = "app:disconnect:";
pub const OPEN_HELP_DOC_EVENT: &str = "open_help_doc";
pub const OPEN_QQ_GROUP_EVENT: &str = "open_qq_group";

// ========== 颜色常量 ==========

const COLOR_TEXT_PRIMARY: &str = "#f4f4f5";
const COLOR_TEXT_SECONDARY: &str = "#a1a1aa";
const COLOR_TEXT_MUTED: &str = "#71717a";
const COLOR_TEXT_DANGER: &str = "#f87171";
const COLOR_BTN_PRIMARY_BG: &str = "#2563eb";
const COLOR_BTN_GHOST_BG: &str = "#27272a";
const COLOR_BTN_DANGER_BG: &str = "#3f1d1d";
const COLOR_STATUS_GREEN: &str = "#4ade80";
const COLOR_STATUS_YELLOW: &str = "#fbbf24";
const COLOR_STATUS_GRAY: &str = "#71717a";
const COLOR_STATUS_RED: &str = "#f87171";

// ========== 渲染入口 ==========

pub fn render_main_ui(element_id: &str) {
    state::set_root(element_id);
    rerender();
}

pub fn rerender() {
    super::event_handler::auto_refresh();
    render_without_auto_refresh();
}

pub fn render_without_auto_refresh() {
    let Some(root) = state::root() else {
        return;
    };
    psys_host::ui_v3::render(&root, build_main_ui());
}

// ========== 主UI ==========

fn build_main_ui() -> ui::Element {
    let current_tab = state::with_state(|s| s.current_tab);
    let tabs = build_tabs(current_tab);
    let content = match current_tab {
        MainTab::Connect => build_connect_tab(),
        MainTab::About => build_about_tab(),
    };

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .padding(20)
        .gap(16)
        .child(tabs)
        .child(content)
}

// ========== Tab栏 ==========

fn build_tabs(current_tab: MainTab) -> ui::Element {
    let tabs_root = ui::Element::new(ui::ElementType::TabsRoot, None)
        .flex()
        .justify_center()
        .margin_bottom(8);

    let tabs_list = ui::Element::new(ui::ElementType::TabsList, None)
        .flex()
        .bg("#1E1E1F")
        .radius(999)
        .padding(4)
        .gap(4);

    let connect_trigger = build_tab_trigger(
        "连接",
        icons::connect_tab_svg(),
        current_tab == MainTab::Connect,
        TAB_CONNECT_EVENT,
    );
    let about_trigger = build_tab_trigger(
        "关于",
        icons::about_tab_svg(),
        current_tab == MainTab::About,
        TAB_ABOUT_EVENT,
    );

    tabs_root.child(tabs_list.child(connect_trigger).child(about_trigger))
}

fn build_tab_trigger(label: &str, icon_svg: String, is_active: bool, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22);
    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

    ui::Element::new(ui::ElementType::TabsTrigger, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(999)
        .padding_top(10)
        .padding_bottom(10)
        .padding_left(14)
        .padding_right(14)
        .bg(if is_active { "#2A2A2A" } else { "#1E1E1F" })
        .text_color(if is_active { "#FFFFFF" } else { "#BBBBBB" })
        .flex()
        .align_center()
        .gap(5)
        .child(icon)
        .child(text)
}

// ========== 连接Tab ==========

fn build_connect_tab() -> ui::Element {
    let devices = state::connected_devices();
    let installed = state::installed_apps();
    let devices_loading = state::is_devices_loading();
    let apps_loading = state::is_apps_loading();

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(16)
        .child(device_section(&devices, devices_loading))
        .child(app_list_section(&installed, apps_loading))
}

// ----- 设备卡片 -----

fn device_section(devices: &[(String, String)], loading: bool) -> ui::Element {
    // 标题行："当前设备:" 左，刷新按钮右
    let title = ui::Element::new(ui::ElementType::P, Some("当前设备:"))
        .size(16)
        .text_color(COLOR_TEXT_PRIMARY)
        .flex_shrink(0.0);

    let spacer = ui::Element::new(ui::ElementType::Div, None).flex_grow(1.0);

    let refresh_text = if loading { "刷新中..." } else { "刷新" };
    let refresh_btn = ghost_button(refresh_text).on(ui::Event::Click, EVENT_REFRESH_DEVICES);

    let header = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8)
        .child(title)
        .child(spacer)
        .child(refresh_btn);

    let mut col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8)
        .child(header);

    if devices.is_empty() {
        col = col.child(device_card_placeholder(loading));
    } else {
        for (addr, name) in devices {
            col = col.child(device_card(name, addr));
        }
    }

    col
}

/// 设备卡片：左侧一个设备图标，右侧两行（名称 / 灰色蓝牙地址）
fn device_card(name: &str, addr: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icons::device_svg()))
        .width(28)
        .height(28)
        .text_color(COLOR_TEXT_SECONDARY);

    let display_name = if name.is_empty() { "未知设备" } else { name };
    let name_el = ui::Element::new(ui::ElementType::P, Some(display_name))
        .size(15)
        .text_color(COLOR_TEXT_PRIMARY);

    let addr_el = ui::Element::new(ui::ElementType::P, Some(addr))
        .size(12)
        .text_color(COLOR_TEXT_MUTED);

    let info_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(2)
        .flex_grow(1.0)
        .child(name_el)
        .child(addr_el);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(16)
        .gap(12)
        .child(icon)
        .child(info_col)
}

/// 无设备时的占位卡片
fn device_card_placeholder(loading: bool) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icons::device_svg()))
        .width(28)
        .height(28)
        .text_color(COLOR_TEXT_MUTED);

    let name_el = ui::Element::new(ui::ElementType::P, Some("未连接设备"))
        .size(15)
        .text_color(COLOR_TEXT_MUTED);

    let hint = if loading {
        "正在搜索设备..."
    } else {
        "请在 AstroBox 中连接手表/手环"
    };
    let hint_el = ui::Element::new(ui::ElementType::P, Some(hint))
        .size(12)
        .text_color(COLOR_TEXT_MUTED);

    let info_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(2)
        .flex_grow(1.0)
        .child(name_el)
        .child(hint_el);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(16)
        .gap(12)
        .child(icon)
        .child(info_col)
}

// ----- 设备应用列表 -----

fn app_list_section(installed: &[InstalledApp], loading: bool) -> ui::Element {
    let title = ui::Element::new(ui::ElementType::P, Some("设备应用列表"))
        .size(16)
        .text_color(COLOR_TEXT_PRIMARY)
        .flex_shrink(0.0);

    let spacer = ui::Element::new(ui::ElementType::Div, None).flex_grow(1.0);

    let refresh_text = if loading { "刷新中..." } else { "刷新" };
    let refresh_btn = ghost_button(refresh_text).on(ui::Event::Click, EVENT_REFRESH_DEVICES);

    let header = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8)
        .child(title)
        .child(spacer)
        .child(refresh_btn);

    let mut col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(8)
        .child(header);

    if installed.is_empty() {
        let hint = if loading {
            "正在加载应用列表..."
        } else {
            "尚未读到设备应用列表，请确认设备已连接。"
        };
        col = col.child(hint_text(hint));
        return col;
    }

    for (idx, app) in installed.iter().enumerate() {
        col = col.child(app_row(app, idx));
    }

    col
}

/// 单个应用行（四态）
fn app_row(app: &InstalledApp, idx: usize) -> ui::Element {
    let status = state::connection_status(&app.addr, &app.package_name);
    let conn = state::connection(&app.addr, &app.package_name);

    let (dot_color, _) = status_color(status);

    // 状态圆点
    let dot = ui::Element::new(ui::ElementType::Div, None)
        .width(8)
        .height(8)
        .radius(4)
        .bg(dot_color)
        .flex_shrink(0.0);

    // 应用名称
    let display_name = if app.app_name.is_empty() {
        app.package_name.as_str()
    } else {
        app.app_name.as_str()
    };
    let name_el = ui::Element::new(ui::ElementType::P, Some(display_name))
        .size(15)
        .text_color(COLOR_TEXT_PRIMARY);

    // 灰色包名
    let pkg_el = ui::Element::new(ui::ElementType::P, Some(&app.package_name))
        .size(12)
        .text_color(COLOR_TEXT_MUTED);

    // 第一行：名称（圆点移到卡片行级，相对整张卡片垂直居中）
    let row1 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .gap(8)
        .child(name_el);

    // 第二行：包名
    let row2 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .child(pkg_el);

    // 左侧信息列
    let mut info_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(4)
        .flex_grow(1.0)
        .child(row1)
        .child(row2);

    // 连接后展开详细信息
    if let Some(ref c) = conn {
        match status {
            AppConnectionStatus::Connected => {
                let stats = format!(
                    "请求 {} · 成功 {} · 失败 {}",
                    c.request_count, c.success_count, c.error_count
                );
                info_col = info_col.child(detail_text(&stats));
                if let Some(ref st) = c.last_status {
                    info_col = info_col.child(detail_text(&format!("最近状态: {}", st)));
                }
            }
            AppConnectionStatus::Failed => {
                if let Some(ref reason) = c.fail_reason {
                    info_col = info_col.child(danger_text(reason));
                }
            }
            _ => {}
        }
    }

    // 右侧按钮
    let button = match status {
        AppConnectionStatus::Connected => {
            danger_button("断开").on(
                ui::Event::Click,
                &format!("{}{}", APP_DISCONNECT_PREFIX, idx),
            )
        }
        AppConnectionStatus::Handshaking => primary_button("连接中…").disabled(),
        AppConnectionStatus::Failed => primary_button("重试").on(
            ui::Event::Click,
            &format!("{}{}", APP_CONNECT_PREFIX, idx),
        ),
        AppConnectionStatus::Disconnected => primary_button("连接").on(
            ui::Event::Click,
            &format!("{}{}", APP_CONNECT_PREFIX, idx),
        ),
    };

    let action_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .align_center()
        .justify_end()
        .child(button);

    // 圆点作为卡片行的直接子元素，配合 align_center 相对整张卡片（含展开信息）垂直居中
    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(12)
        .child(dot)
        .child(info_col)
        .child(action_col)
}

fn status_color(status: AppConnectionStatus) -> (&'static str, &'static str) {
    match status {
        AppConnectionStatus::Disconnected => (COLOR_STATUS_GRAY, "未连接"),
        AppConnectionStatus::Handshaking => (COLOR_STATUS_YELLOW, "握手中"),
        AppConnectionStatus::Connected => (COLOR_STATUS_GREEN, "已连接"),
        AppConnectionStatus::Failed => (COLOR_STATUS_RED, "失败"),
    }
}

// ========== 关于Tab ==========

fn build_about_tab() -> ui::Element {
    let more_title = build_section_title("更多内容");

    let help_card = build_settings_card(
        icons::help_svg(),
        "帮助文档",
        Some("操作步骤与常见问题解答"),
        Some(build_more_link_icon()),
        Some(OPEN_HELP_DOC_EVENT),
    );

    let qq_card = build_settings_card(
        icons::qq_group_svg(),
        "QQ交流群",
        Some("1076096725"),
        Some(build_more_link_icon()),
        Some(OPEN_QQ_GROUP_EVENT),
    );

    let build_title = build_section_title("构建信息");

    let build_time_raw = option_env!("AB_BUILD_TIME").unwrap_or("unknown");
    let build_user = option_env!("AB_BUILD_USER").unwrap_or("unknown");
    let build_branch = option_env!("AB_BUILD_GIT_BRANCH").unwrap_or("unknown");
    let build_hash = short_git_hash(option_env!("AB_BUILD_GIT_HASH").unwrap_or("unknown"));
    let build_time = format_beijing_time(build_time_raw);

    let build_time_row = build_settings_card(
        icons::time_svg(),
        "构建时间",
        None,
        Some(build_value_text(&build_time)),
        None,
    );
    let build_user_row = build_settings_card(
        icons::user_svg(),
        "构建用户",
        None,
        Some(build_value_text(build_user)),
        None,
    );
    let build_branch_row = build_settings_card(
        icons::branch_svg(),
        "当前分支",
        None,
        Some(build_value_text(build_branch)),
        None,
    );
    let build_hash_row = build_settings_card(
        icons::hash_svg(),
        "当前hash",
        None,
        Some(build_value_text(&build_hash)),
        None,
    );

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8)
        .child(more_title)
        .child(help_card)
        .child(qq_card)
        .child(build_title)
        .child(build_time_row)
        .child(build_user_row)
        .child(build_branch_row)
        .child(build_hash_row)
}

// ========== 辅助组件 ==========

fn build_settings_card(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22);
    let icon_wrap = ui::Element::new(ui::ElementType::Div, None)
        .width(22)
        .height(22)
        .flex()
        .align_center()
        .justify_center()
        .child(icon);

    let mut text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    text_col = text_col.child(
        ui::Element::new(ui::ElementType::P, Some(title))
            .size(15)
            .text_color(COLOR_TEXT_PRIMARY),
    );

    if let Some(desc_text) = desc {
        text_col = text_col.child(
            ui::Element::new(ui::ElementType::P, Some(desc_text))
                .size(13)
                .text_color(COLOR_TEXT_SECONDARY),
        );
    }

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .gap(10)
        .child(icon_wrap)
        .child(text_col);

    if let Some(right_el) = right {
        let right_wrap = ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .align_center()
            .justify_end()
            .child(right_el);
        row = row.child(right_wrap);
    }

    if let Some(event_id) = click_event {
        row = row.on(ui::Event::Click, event_id);
    }

    row
}

fn build_section_title(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(13)
        .text_color("#888888")
        .margin_left(12)
        .margin_top(10)
}

fn build_more_link_icon() -> ui::Element {
    let svg = icons::more_link_svg();
    ui::Element::new(ui::ElementType::Svg, Some(&svg))
        .width(18)
        .height(18)
        .text_color("#0088FF")
}

fn build_value_text(value: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(value))
        .size(13)
        .text_color("#BBBBBB")
}

fn primary_button(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .without_default_styles()
        .padding_top(8)
        .padding_bottom(8)
        .padding_left(16)
        .padding_right(16)
        .radius(8)
        .bg(COLOR_BTN_PRIMARY_BG)
        .text_color(COLOR_TEXT_PRIMARY)
        .size(13)
}

fn ghost_button(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .without_default_styles()
        .padding_left(12)
        .padding_right(12)
        .padding_top(6)
        .padding_bottom(6)
        .radius(8)
        .bg(COLOR_BTN_GHOST_BG)
        .text_color(COLOR_TEXT_SECONDARY)
        .size(14)
}

fn danger_button(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, Some(text))
        .without_default_styles()
        .padding_top(8)
        .padding_bottom(8)
        .padding_left(16)
        .padding_right(16)
        .radius(8)
        .bg(COLOR_BTN_DANGER_BG)
        .text_color(COLOR_TEXT_DANGER)
        .size(13)
}

fn hint_text(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(12)
        .text_color(COLOR_TEXT_MUTED)
        .margin_top(8)
}

fn detail_text(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(11)
        .text_color(COLOR_TEXT_SECONDARY)
}

fn danger_text(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(11)
        .text_color(COLOR_TEXT_DANGER)
}

// ========== 时间工具 ==========

fn format_beijing_time(raw: &str) -> String {
    if let Some((y, m, d, hh, mm, ss)) = parse_iso_utc(raw) {
        let (y2, m2, d2, hh2) = add_hours(y, m, d, hh, 8);
        return format!(
            "{:04}‑{:02}‑{:02}_{:02}:{:02}:{:02}",
            y2, m2, d2, hh2, mm, ss
        );
    }
    raw.to_string()
}

fn parse_iso_utc(raw: &str) -> Option<(i32, i32, i32, i32, i32, i32)> {
    if raw.len() < 19 {
        return None;
    }
    let base = &raw[..19];
    let mut parts = base.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut dparts = date.split('-');
    let y: i32 = dparts.next()?.parse().ok()?;
    let m: i32 = dparts.next()?.parse().ok()?;
    let d: i32 = dparts.next()?.parse().ok()?;
    let mut tparts = time.split(':');
    let hh: i32 = tparts.next()?.parse().ok()?;
    let mm: i32 = tparts.next()?.parse().ok()?;
    let ss: i32 = tparts.next()?.parse().ok()?;
    Some((y, m, d, hh, mm, ss))
}

fn add_hours(mut y: i32, mut m: i32, mut d: i32, mut hh: i32, add: i32) -> (i32, i32, i32, i32) {
    hh += add;
    while hh >= 24 {
        hh -= 24;
        d += 1;
        let dim = days_in_month(y, m);
        if d > dim {
            d = 1;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
    }
    (y, m, d, hh)
}

fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn short_git_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    if trimmed.is_empty() || trimmed == "unknown" {
        return "unknown".to_string();
    }
    trimmed.chars().take(7).collect()
}
