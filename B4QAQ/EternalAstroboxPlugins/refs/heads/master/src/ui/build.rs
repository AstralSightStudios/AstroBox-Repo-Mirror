use super::SYNC_CARD_ID;
use super::event_handler::*;
use super::icons;
use super::state::*;
use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui_v3 as ui;

/// 渲染主UI
pub fn render_main_ui(element_id: &str) {
    {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id = Some(element_id.to_string());
    }

    crate::ui::state::load_api_settings_once();

    let ui_tree = build_main_ui();
    psys_host::ui_v3::render(element_id, ui_tree);
}

/// 构建主UI
pub fn build_main_ui() -> ui::Element {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .padding(20);

    // 如果未验证，不显示Tab，直接显示验证界面
    if !state.api_key_verified {
        let verification_ui = build_verification_ui(&state);
        return container.child(verification_ui);
    }

    let tabs = build_tabs(&state);
    let content = match state.current_tab {
        MainTab::SyncData => build_sync_tab(&state),
        MainTab::CityManage => build_city_manage_tab(&state),
        MainTab::Notice => build_notice_tab(&state),
        MainTab::Settings => build_settings_tab(&state),
    };

    container.child(tabs).child(content)
}

/// 渲染同步卡片
pub fn render_sync_card(card_id: &str) {
    tracing::info!("render_sync_card called: card_id={}", card_id);
    if card_id != SYNC_CARD_ID {
        tracing::info!(
            "render_sync_card id mismatch: expected {}, got {}",
            SYNC_CARD_ID,
            card_id
        );
    }
    let text = build_sync_card_text();
    tracing::info!("render_sync_card content: {}", text);
    psys_host::ui_v3::render_to_text_card(card_id, &text);
}

// ========== Tab 构建函数 ==========

fn build_tabs(state: &UiState) -> ui::Element {
    let tabs_root = ui::Element::new(ui::ElementType::TabsRoot, None)
        .flex()
        .justify_center()
        .margin_bottom(20);

    let tabs_list = ui::Element::new(ui::ElementType::TabsList, None)
        .flex()
        .bg("#1E1E1F")
        .radius(999)
        .padding(4)
        .gap(4);

    let sync_trigger = build_tab_trigger(
        "同步数据",
        icons::send_tab_svg(),
        state.current_tab == MainTab::SyncData,
        TAB_SYNC_EVENT,
    );

    let city_trigger = build_tab_trigger(
        "城市管理",
        icons::city_svg(),
        state.current_tab == MainTab::CityManage,
        TAB_CITY_EVENT,
    );

    let notice_trigger = build_tab_trigger(
        "公告",
        icons::notice_svg(),
        state.current_tab == MainTab::Notice,
        TAB_NOTICE_EVENT,
    );

    let settings_trigger = build_tab_trigger(
        "设置",
        icons::api_tab_svg(),
        state.current_tab == MainTab::Settings,
        TAB_SETTINGS_EVENT,
    );

    tabs_root
        .child(tabs_list.child(sync_trigger).child(city_trigger).child(notice_trigger).child(settings_trigger))
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

// ========== 同步数据Tab ==========

fn build_sync_tab(state: &UiState) -> ui::Element {
    if !state.api_key_verified {
        return build_verification_ui(state);
    }

    build_weather_sync_ui(state)
}

fn build_verification_ui(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .align_center()
        .gap(16);

    match state.verification_status {
        VerificationStatus::NotStarted => {
            // 欢迎使用永昼天气
            let title = ui::Element::new(ui::ElementType::P, Some("欢迎使用永昼天气"))
                .size(24)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            let subtitle = ui::Element::new(ui::ElementType::P, Some("接下来开始进行验证"))
                .size(14)
                .text_color("#888888")
                .margin_bottom(20);

            let verify_button = build_icon_text_button_full(
                "验证",
                icons::api_tab_svg(),
                UPGRADE_TO_PAID_EVENT,
            ).bg("#0090FF").text_color("#FFFFFF");

            root.child(title).child(subtitle).child(verify_button)
        }

        VerificationStatus::CheckingDevice => {
            // 检测设备链接
            let title = ui::Element::new(ui::ElementType::P, Some("检测设备链接"))
                .size(24)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            let subtitle = ui::Element::new(ui::ElementType::P, Some("请选择您的设备"))
                .size(14)
                .text_color("#888888")
                .margin_bottom(16);

            // 设备信息卡片（使用主机 device 接口的 device-info）
            let device_card = if let Some((ref name, ref addr)) = state.host_device_info {
                build_device_info_card_for_verification(name, addr)
            } else {
                ui::Element::new(ui::ElementType::P, Some("正在搜索设备..."))
                    .size(14)
                    .text_color("#666666")
            };

            root.child(title).child(subtitle).child(device_card)
        }

        VerificationStatus::GettingAPIKey | VerificationStatus::GettingDeviceInfo => {
            // 正在获取信息
            let title = ui::Element::new(ui::ElementType::P, Some("正在获取信息"))
                .size(24)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            let subtitle = ui::Element::new(ui::ElementType::P, Some("请稍后"))
                .size(14)
                .text_color("#888888");

            root.child(title).child(subtitle)
        }

        VerificationStatus::WaitingPayment => {
            // 设备激活
            let title = ui::Element::new(ui::ElementType::P, Some("设备激活"))
                .size(24)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            let you_device = ui::Element::new(ui::ElementType::P, Some("您选择的设备:"))
                .size(14)
                .text_color("#888888")
                .margin_bottom(8);

            // 设备信息卡片（使用主机 device 接口的 device-info）
            let device_card = if let Some((ref name, ref addr)) = state.host_device_info {
                build_device_info_card_for_verification(name, addr)
            } else {
                ui::Element::new(ui::ElementType::P, Some("设备信息获取中..."))
                    .size(14)
                    .text_color("#666666")
            };

            // 分隔
            let spacer = ui::Element::new(ui::ElementType::Div, None)
                .height(16);

            // 付费版激活提示
            let paid_title = ui::Element::new(ui::ElementType::P, Some("付费版激活"))
                .size(16)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            // 跳转支付页面按钮（用户手动点击）
            let pay_button = build_icon_text_button_full(
                "前往支付页面",
                icons::more_link_svg(),
                OPEN_VERIFY_URL_EVENT,
            ).bg("#0090FF").text_color("#FFFFFF").margin_bottom(8);

            // 已支付提示
            let paid_hint = ui::Element::new(ui::ElementType::P, Some("支付完成后点击下方按钮验证"))
                .size(13)
                .text_color("#888888")
                .margin_bottom(8);

            // 验证支付按钮
            let verify_paid_button = build_icon_text_button_full(
                "验证支付",
                icons::refresh_auth_svg(),
                CHECK_PAYMENT_EVENT,
            ).bg("#FF9800").text_color("#FFFFFF").margin_bottom(16);

            // 免费版分隔
            let free_title = ui::Element::new(ui::ElementType::P, Some("免费版激活"))
                .size(16)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            let free_hint = ui::Element::new(ui::ElementType::P, Some("免费版有请求次数限制"))
                .size(13)
                .text_color("#888888")
                .margin_bottom(8);

            // 免费版按钮
            let free_button = build_icon_text_button_full(
                "免费激活",
                icons::afd_svg(),
                FREE_VERSION_EVENT,
            ).bg("#4CAF50").text_color("#FFFFFF");

            root.child(title)
                .child(you_device)
                .child(device_card)
                .child(spacer)
                .child(paid_title)
                .child(pay_button)
                .child(paid_hint)
                .child(verify_paid_button)
                .child(free_title)
                .child(free_hint)
                .child(free_button)
        }

        VerificationStatus::VerifyingPayment => {
            // 验证中
            let title = ui::Element::new(ui::ElementType::P, Some("验证中,请稍后"))
                .size(24)
                .text_color("#FFFFFF")
                .margin_bottom(8);

            let subtitle = ui::Element::new(ui::ElementType::P, Some("正在验证您的订单,并下发Key至设备"))
                .size(14)
                .text_color("#888888");

            root.child(title).child(subtitle)
        }

        VerificationStatus::Verified => {
            // 一切就绪
            let title = ui::Element::new(ui::ElementType::P, Some("一切就绪！"))
                .size(24)
                .text_color("#4CAF50")
                .margin_bottom(8);

            let subtitle = ui::Element::new(ui::ElementType::P, Some("开始使用吧"))
                .size(14)
                .text_color("#888888");

            root.child(title).child(subtitle)
        }

        VerificationStatus::Failed => {
            // 失败弹窗由 show_alert 处理，这里显示重试界面
            let title = ui::Element::new(ui::ElementType::P, Some("验证失败"))
                .size(24)
                .text_color("#F44336")
                .margin_bottom(8);

            let subtitle = ui::Element::new(ui::ElementType::P, Some("请点击重试"))
                .size(14)
                .text_color("#888888")
                .margin_bottom(20);

            let retry_button = build_icon_text_button_full(
                "重试",
                icons::refresh_svg(),
                UPGRADE_TO_PAID_EVENT,
            ).bg("#0090FF").text_color("#FFFFFF");

            root.child(title).child(subtitle).child(retry_button)
        }
    }
}

/// 构建验证流程中的设备信息卡片
/// 构建设备信息卡片（用于激活页面，使用主机 device 接口的 device-info）
fn build_device_info_card_for_verification(name: &str, addr: &str) -> ui::Element {
    let card = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(16);

    // 设备名称（来自 device::get_connected_device_list() 的 device-info.name）
    let display_name = if name.is_empty() {
        "未知设备".to_string()
    } else {
        name.to_string()
    };
    let name_label = ui::Element::new(ui::ElementType::P, Some(&display_name))
        .size(15)
        .text_color("#FFFFFF");

    let spacer = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0);

    // 设备地址（来自 device::get_connected_device_list() 的 device-info.addr）
    let display_addr = if addr.is_empty() {
        "未知地址".to_string()
    } else {
        addr.to_string()
    };
    let addr_label = ui::Element::new(ui::ElementType::P, Some(&display_addr))
        .size(13)
        .text_color("#888888");

    card.child(name_label).child(spacer).child(addr_label)
}

fn build_weather_sync_ui(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    // 选择城市下拉菜单（参考 simple-weather 的 Select 样式）
    let city_label = ui::Element::new(ui::ElementType::P, Some("选择城市"))
        .size(15)
        .margin_bottom(8);

    let selected_city_name = if let Some(idx) = state.selected_city_index {
        if idx < state.city_list.len() {
            let city = &state.city_list[idx];
            if city.adm1.is_empty() {
                city.name.clone()
            } else {
                format!("{} · {}", city.name, city.adm1)
            }
        } else {
            "请选择城市".to_string()
        }
    } else if !state.city_list.is_empty() {
        // 如果没有选中但列表不为空，默认选中第一个
        let city = &state.city_list[0];
        if city.adm1.is_empty() {
            city.name.clone()
        } else {
            format!("{} · {}", city.name, city.adm1)
        }
    } else {
        "请选择城市".to_string()
    };

    let mut city_select = ui::Element::new(ui::ElementType::Select, Some(&selected_city_name))
        .on(ui::Event::Change, SELECT_CITY_DROPDOWN_EVENT)
        .radius(8)
        .padding_left(12)
        .padding_right(12)
        .bg("#2A2A2A")
        .size(14);

    if state.city_list.is_empty() {
        let option = ui::Element::new(ui::ElementType::Option, Some("请先添加城市"));
        city_select = city_select.child(option);
    } else {
        for city in state.city_list.iter() {
            let label = if city.adm1.is_empty() {
                city.name.clone()
            } else {
                format!("{} · {}", city.name, city.adm1)
            };
            let option = ui::Element::new(ui::ElementType::Option, Some(&label));
            city_select = city_select.child(option);
        }
    }

    let days_card = build_days_card(state).margin_top(10);

    let alerts_card = build_settings_card(
        icons::alerts_svg(),
        "同步天气预警数据",
        Some("开启后同步天气预警灾害信息"),
        Some(build_switch(state.sync_alerts_enabled, ALERTS_SYNC_TOGGLE_EVENT)),
        None,
    ).margin_bottom(10);

    // 同步按钮（根据同步状态显示不同内容）
    let send_button = if state.sync_progress.syncing {
        // 显示进度
        let progress_text = format!(
            "同步中 {}/{}",
            state.sync_progress.current_day,
            state.sync_progress.total_days
        );
        build_icon_text_button_full(
            &progress_text,
            icons::refresh_svg(),
            "", // 空事件，禁用点击
        ).bg("#FF980026").text_color("#FF9800")
    } else {
        build_icon_text_button_full(
            "同步数据",
            icons::send_tab_svg(),
            SEND_BUTTON_EVENT,
        ).bg("#0090FF26").text_color("#0090FF")
    };

    root.child(city_label)
        .child(city_select)
        .child(days_card)
        .child(alerts_card)
        .child(send_button)
}

// ========== 公告Tab ==========

fn build_notice_tab(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    // 刷新按钮
    let refresh_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .justify_end()
        .width_full()
        .margin_bottom(8);

    let refresh_text = if state.notice_loading { "刷新中..." } else { "刷新" };
    let refresh_btn = ui::Element::new(ui::ElementType::Button, Some(refresh_text))
        .without_default_styles()
        .on(ui::Event::Click, REFRESH_NOTICE_EVENT)
        .bg("#2A2A2A")
        .text_color("#FFFFFF")
        .radius(8)
        .padding_left(12)
        .padding_right(12)
        .padding_top(6)
        .padding_bottom(6)
        .size(14);

    let refresh_row = refresh_row.child(refresh_btn);

    // 公告列表
    let content = if state.notice_loading {
        ui::Element::new(ui::ElementType::P, Some("加载中..."))
            .size(14)
            .text_color("#888888")
            .margin_top(20)
    } else if state.notice_list.is_empty() {
        ui::Element::new(ui::ElementType::P, Some("暂无公告"))
            .size(14)
            .text_color("#888888")
            .margin_top(20)
    } else {
        let mut container = ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Column)
            .gap(12);

        for notice in &state.notice_list {
            let card = build_notice_card(notice);
            container = container.child(card);
        }
        container
    };

    root.child(refresh_row).child(content)
}

/// 构建单个公告卡片
fn build_notice_card(notice: &NoticeInfo) -> ui::Element {
    let card = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(8);

    // 标题行：标题 + 类型标签
    let title_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8);

    let title_text = ui::Element::new(ui::ElementType::P, Some(&notice.title))
        .size(15)
        .flex_shrink(0.0);

    let spacer = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0);

    // 类型标签颜色
    let type_color = match notice.notice_type.as_str() {
        "warning" => "#FF9800",
        "error" => "#F44336",
        _ => "#2196F3", // info
    };
    let type_bg = match notice.notice_type.as_str() {
        "warning" => "#FF980026",
        "error" => "#F4433626",
        _ => "#2196F326",
    };
    let type_label = ui::Element::new(ui::ElementType::Span, Some(&notice.notice_type))
        .bg(type_bg)
        .text_color(type_color)
        .radius(4)
        .padding_left(6)
        .padding_right(6)
        .padding_top(2)
        .padding_bottom(2)
        .size(12);

    // 时间
    let time_text = ui::Element::new(ui::ElementType::P, Some(&notice.time))
        .size(12)
        .text_color("#888888");

    // 内容（解析特殊格式）
    let segments = parse_notice_content(&notice.content);
    let mut content_container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .gap(4);

    for segment in segments {
        let seg_el = match segment {
            NoticeSegment::Text { text } => {
                // 处理换行：按 \n 分割成多行
                let lines: Vec<&str> = text.split('\n').collect();
                if lines.len() == 1 {
                    ui::Element::new(ui::ElementType::P, Some(&text))
                        .size(14)
                        .text_color("#CCCCCC")
                } else {
                    let mut lines_container = ui::Element::new(ui::ElementType::Div, None)
                        .flex()
                        .flex_direction(ui::FlexDirection::Column)
                        .gap(2);
                    for line in lines {
                        if !line.is_empty() {
                            let line_el = ui::Element::new(ui::ElementType::P, Some(line))
                                .size(14)
                                .text_color("#CCCCCC");
                            lines_container = lines_container.child(line_el);
                        }
                    }
                    lines_container
                }
            }
            NoticeSegment::Image { url, alt } => {
                // 图片显示为可点击链接
                ui::Element::new(ui::ElementType::Button, Some(&format!("[图片: {}]", alt)))
                    .without_default_styles()
                    .on(ui::Event::Click, &format!("{}{}", OPEN_NOTICE_LINK_PREFIX, url))
                    .text_color("#0090FF")
                    .size(14)
                    .padding(0)
            }
            NoticeSegment::QrCode { url, alt } => {
                // 二维码显示为可点击链接
                ui::Element::new(ui::ElementType::Button, Some(&format!("[二维码: {}]", alt)))
                    .without_default_styles()
                    .on(ui::Event::Click, &format!("{}{}", OPEN_NOTICE_LINK_PREFIX, url))
                    .text_color("#0090FF")
                    .size(14)
                    .padding(0)
            }
        };
        content_container = content_container.child(seg_el);
    }

    card.child(title_row.child(title_text).child(spacer).child(type_label))
        .child(time_text)
        .child(content_container)
}

/// 解析公告内容，提取文本、图片、二维码
fn parse_notice_content(content: &str) -> Vec<NoticeSegment> {
    use regex::Regex;

    let mut segments = Vec::new();
    // 匹配 [IMG:url,alt] 或 [QR:url,alt] 或 [QRCODE:url,alt]
    let re = Regex::new(r"\[(?:QR(?:CODE)?:|IMG:)\s*([^,\]]+?)\s*,\s*([^,\]]+?)\s*\]").unwrap();

    let mut last_end = 0;
    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();

        // 匹配前的文本
        if full_match.start() > last_end {
            let text = &content[last_end..full_match.start()];
            if !text.is_empty() {
                segments.push(NoticeSegment::Text { text: text.to_string() });
            }
        }

        // 匹配的内容
        let full_text = full_match.as_str();
        let is_img = full_text.starts_with("[IMG");
        let url = cap[1].trim().to_string();
        let alt = cap[2].trim().to_string();

        if is_img {
            segments.push(NoticeSegment::Image { url, alt });
        } else {
            segments.push(NoticeSegment::QrCode { url, alt });
        }

        last_end = full_match.end();
    }

    // 剩余文本
    if last_end < content.len() {
        let text = &content[last_end..];
        if !text.is_empty() {
            segments.push(NoticeSegment::Text { text: text.to_string() });
        }
    }

    // 如果没有特殊格式，直接返回原文
    if segments.is_empty() {
        segments.push(NoticeSegment::Text { text: content.to_string() });
    }

    segments
}

// ========== 城市管理Tab ==========

const INPUT_HEIGHT: u32 = 40;

fn build_city_manage_tab(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    // 搜索行：输入框 + 搜索按钮（参考 simple-weather-astrobox-v2-plugin）
    let search_input = ui::Element::new(ui::ElementType::Input, Some(&state.city_search_keyword))
        .on(ui::Event::Input, "city_search_input")
        .radius(18)
        .bg("#2A2A2A")
        .height(INPUT_HEIGHT)
        .width_full()
        .padding_left(12)
        .padding_right(12)
        .flex_grow(1.0);

    // 搜索按钮（图标样式）
    let search_icon = ui::Element::new(ui::ElementType::Svg, Some(&icons::search_svg()))
        .width(16)
        .height(16);

    let search_button = ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, SEARCH_CITY_BUTTON_EVENT)
        .radius(18)
        .height(INPUT_HEIGHT)
        .padding_left(10)
        .padding_right(10)
        .bg("#2A2A2A")
        .width(44)
        .flex()
        .align_center()
        .justify_center()
        .child(search_icon);

    let search_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8)
        .child(search_input)
        .child(search_button);

    // 搜索结果列表（可折叠）
    let search_results = if state.city_search_loading {
        let loading_text = ui::Element::new(ui::ElementType::P, Some("搜索中..."))
            .size(14)
            .text_color("#888888")
            .margin_top(8);
        loading_text
    } else if !state.city_search_results.is_empty() {
        // 折叠标题行
        let collapse_icon = if state.search_results_expanded { "▼ " } else { "▶ " };
        let count_text = format!("{}搜索结果 ({} 个)", collapse_icon, state.city_search_results.len());

        let collapse_header = ui::Element::new(ui::ElementType::Button, Some(&count_text))
            .without_default_styles()
            .on(ui::Event::Click, TOGGLE_SEARCH_RESULTS_EVENT)
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .align_center()
            .width_full()
            .margin_top(8)
            .padding(8)
            .bg("#1E1E1F")
            .radius(8)
            .text_color("#BBBBBB");

        if state.search_results_expanded {
            let mut container = ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Column)
                .gap(8)
                .margin_top(4);

            for (idx, city) in state.city_search_results.iter().enumerate() {
                let item = build_city_search_item(city, idx);
                container = container.child(item);
            }
            let wrapper = ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Column)
                .child(collapse_header)
                .child(container);
            wrapper
        } else {
            collapse_header
        }
    } else if !state.city_search_keyword.is_empty() {
        let no_result = ui::Element::new(ui::ElementType::P, Some("未找到匹配的城市"))
            .size(14)
            .text_color("#888888")
            .margin_top(8);
        no_result
    } else {
        ui::Element::new(ui::ElementType::Div, None)
    };

    // 城市列表标题行（包含刷新按钮）
    let list_title = ui::Element::new(ui::ElementType::P, Some("城市列表"))
        .size(16)
        .flex_shrink(0.0);

    let spacer = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0);

    // 刷新按钮（根据加载状态显示不同文字）
    let refresh_text = if state.city_list_loading {
        "刷新中..."
    } else {
        "刷新"
    };

    let refresh_button = ui::Element::new(ui::ElementType::Button, Some(refresh_text))
        .without_default_styles()
        .on(ui::Event::Click, GET_CITYLIST_EVENT)
        .bg("#2A2A2A")
        .text_color("#FFFFFF")
        .radius(8)
        .padding_left(12)
        .padding_right(12)
        .padding_top(6)
        .padding_bottom(6)
        .size(14)
        .flex_shrink(0.0);

    let list_header = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .margin_top(10)
        .gap(8)
        .child(list_title)
        .child(spacer)
        .child(refresh_button);

    // 城市列表
    let city_list_container = if state.city_list.is_empty() {
        ui::Element::new(ui::ElementType::P, Some("暂无城市，请先搜索添加"))
            .size(14)
            .text_color("#888888")
            .margin_top(10)
    } else {
        let mut container = ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Column)
            .gap(8)
            .margin_top(10);

        for (idx, city) in state.city_list.iter().enumerate() {
            let city_item = build_city_item(city, idx, state.selected_city_index == Some(idx));
            container = container.child(city_item);
        }
        container
    };

    root.child(search_row)
        .child(search_results)
        .child(list_header)
        .child(city_list_container)
}

/// 构建城市搜索结果项（两行布局，右侧大添加按钮）
fn build_city_search_item(city: &CityInfo, idx: usize) -> ui::Element {
    // 外层容器
    let item = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(12);

    // 左侧内容（两行）
    let left_content = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .flex_grow(1.0)
        .gap(4);

    // 第一行：城市名
    let name_text = ui::Element::new(ui::ElementType::P, Some(&city.name))
        .size(15)
        .flex_shrink(0.0);

    // 第二行：adm1 adm2
    let adm_text = if city.adm1.is_empty() {
        city.country.clone()
    } else if city.adm2.is_empty() {
        format!("{} {}", city.adm1, city.country)
    } else {
        format!("{} {} {}", city.adm1, city.adm2, city.country)
    };

    let adm_label = ui::Element::new(ui::ElementType::P, Some(&adm_text))
        .size(13)
        .text_color("#888888")
        .flex_shrink(0.0);

    // 右侧大添加按钮（占据两行高度）
    let add_btn = ui::Element::new(ui::ElementType::Button, Some("+"))
        .without_default_styles()
        .on(ui::Event::Click, &format!("{}{}", ADD_CITY_PREFIX, idx))
        .bg("#0090FF")
        .text_color("#FFFFFF")
        .radius(8)
        .width(48)
        .flex_shrink(0.0)
        .flex()
        .align_center()
        .justify_center()
        .size(20);

    item.child(left_content.child(name_text).child(adm_label))
        .child(add_btn)
}

fn build_city_item(city: &CityInfo, idx: usize, is_selected: bool) -> ui::Element {
    let item = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12);

    // 第一行: 城市名 + 排序按钮
    let row1 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8);

    let name_text = ui::Element::new(ui::ElementType::P, Some(&city.name))
        .size(15)
        .text_color(if is_selected { "#0090FF" } else { "#FFFFFF" })
        .flex_shrink(0.0); // 防止被压缩

    let spacer1 = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0); // 占据剩余空间

    // 排序按钮
    let order_up = ui::Element::new(ui::ElementType::Button, Some("↑"))
        .without_default_styles()
        .on(ui::Event::Click, &format!("{}{},-1", CITY_ORDER_PREFIX, idx))
        .bg("#2A2A2A")
        .text_color("#FFFFFF")
        .radius(6)
        .padding_left(10)
        .padding_right(10)
        .padding_top(4)
        .padding_bottom(4)
        .size(12)
        .flex_shrink(0.0);

    let order_down = ui::Element::new(ui::ElementType::Button, Some("↓"))
        .without_default_styles()
        .on(ui::Event::Click, &format!("{}{},1", CITY_ORDER_PREFIX, idx))
        .bg("#2A2A2A")
        .text_color("#FFFFFF")
        .radius(6)
        .margin_left(4)
        .padding_left(10)
        .padding_right(10)
        .padding_top(4)
        .padding_bottom(4)
        .size(12)
        .flex_shrink(0.0);

    // 第二行: adm1 adm2 + 删除按钮
    let row2 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .margin_top(8)
        .gap(8);

    let adm_text = if city.adm1.is_empty() {
        "".to_string()
    } else if city.adm2.is_empty() {
        city.adm1.clone()
    } else {
        format!("{} {}", city.adm1, city.adm2)
    };

    let adm_label = ui::Element::new(ui::ElementType::P, Some(&adm_text))
        .size(15)
        .text_color("#888888")
        .flex_shrink(0.0); // 防止被压缩

    let spacer2 = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0); // 占据剩余空间

    let delete_btn = ui::Element::new(ui::ElementType::Button, Some("删除"))
        .without_default_styles()
        .on(ui::Event::Click, &format!("{}{}", DELETE_CITY_PREFIX, idx))
        .bg("#FF4444")
        .text_color("#FFFFFF")
        .radius(6)
        .padding_left(10)
        .padding_right(10)
        .padding_top(4)
        .padding_bottom(4)
        .size(12)
        .flex_shrink(0.0);

    item.child(row1.child(name_text).child(spacer1).child(order_up).child(order_down))
        .child(row2.child(adm_label).child(spacer2).child(delete_btn))
}

// ========== 设置Tab ==========

fn build_settings_tab(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    // 授权状态标题
    let auth_title = build_section_title("授权状态");

    // 第一个卡片：APIKey状态 + 显示/隐藏
    let api_key_card = build_apikey_status_card(state);

    // 从服务器获取的设备信息
    let mut info_cards = Vec::new();

    if let Some(ref info) = state.server_device_info {
        // API返回的是 {"status":200, "result": {...}}，需要先提取result
        let result = info.get("result").unwrap_or(info);

        // 到期时间（订阅制）
        if let Some(expired_at) = result.get("expiredAt").and_then(|v| v.as_str()) {
            let expire_card = build_settings_card(
                icons::calendar_svg(),
                "到期时间",
                Some(expired_at),
                None,
                None,
            );
            info_cards.push(expire_card);
        }

        // 剩余金额（按量制）
        if let Some(remaining) = result.get("remainingAmount").and_then(|v| v.as_str()) {
            let remain_card = build_settings_card(
                icons::afd_svg(),
                "剩余金额",
                Some(&format!("{} 元", remaining)),
                None,
                None,
            );
            info_cards.push(remain_card);
        }
    }

    // 第二个卡片：请求用量 + 进度条
    let usage_card = build_usage_card(state);

    // 计费模式（放在请求用量下面）
    let mut billing_mode_card = None;
    // 升级为付费版（放在计费模式下面，黄色样式）
    let mut upgrade_card = None;
    if let Some(ref info) = state.server_device_info {
        let result = info.get("result").unwrap_or(info);
        if let Some(billing_mode) = result.get("billingMode").and_then(|v| v.as_str()) {
            billing_mode_card = Some(build_settings_card(
                icons::hash_svg(),
                "计费模式",
                Some(billing_mode),
                None,
                None,
            ));
        }
        // 如果是免费版，显示升级按钮（黄色样式，跳转支付页）
        if let Some(user_type) = result.get("userType").and_then(|v| v.as_str()) {
            if user_type == "free" {
                upgrade_card = Some(build_settings_card_colored(
                    icons::afd_svg(),
                    "升级为付费版",
                    Some("点击跳转至支付页面"),
                    Some(build_more_link_icon()),
                    Some(OPEN_PAY_URL_EVENT),
                    "#FFB30026", // 黄色背景
                    "#FFB300",   // 黄色文字
                ));
            }
        }
    }

    // 刷新设备信息按钮
    let refresh_button = build_icon_text_button_full(
        "刷新授权信息",
        icons::refresh_auth_svg(),
        REFRESH_DEVICE_INFO_EVENT,
    );

    // 搜索设置
    let search_title = build_section_title("搜索设置");

    // 搜索范围选择（不使用 value prop，直接用文本匹配）
    let selected_range_text = if state.city_search_range == "cn" {
        "中国"
    } else if state.city_search_range == "jp" {
        "日本"
    } else {
        "全球"
    };

    let mut range_select = ui::Element::new(ui::ElementType::Select, Some(selected_range_text))
        .on(ui::Event::Change, SEARCH_RANGE_EVENT)
        .radius(8)
        .padding(10)
        .bg("#2A2A2A")
        .size(14);

    // Option 不使用 value，使用非空占位符避免错误
    for label in ["全球", "中国", "日本"] {
        let option = ui::Element::new(ui::ElementType::Option, Some(label))
            .prop("value", label); // 用文本作为 value，避免空字符串
        range_select = range_select.child(option);
    }

    let range_card = build_settings_card(
        icons::search_settings_svg(),
        "搜索范围",
        None,
        Some(range_select),
        None,
    );

    // 结果数量选择
    let selected_number_text = format!("{} 个", state.city_search_number);

    let mut number_select = ui::Element::new(ui::ElementType::Select, Some(&selected_number_text))
        .on(ui::Event::Change, SEARCH_NUMBER_EVENT)
        .radius(8)
        .padding(10)
        .bg("#2A2A2A")
        .size(14);

    for num in [5u32, 10, 15, 20] {
        let option_text = format!("{} 个", num);
        let option = ui::Element::new(ui::ElementType::Option, Some(&option_text))
            .prop("value", &option_text); // 用文本作为 value
        number_select = number_select.child(option);
    }

    let number_card = build_settings_card(
        icons::list_svg(),
        "结果数量",
        None,
        Some(number_select),
        None,
    );

    // 更多内容
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

    // 删除本地授权（红色警告样式）
    let delete_auth_card = build_settings_card_colored(
        icons::refresh_svg(),
        "删除设备本地授权",
        Some("删除所有存储的本地设备信息"),
        None,
        Some(DELETE_LOCAL_AUTH_EVENT),
        "#F4433626", // 红色背景
        "#F44336",   // 红色文字
    );

    // 构建信息
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

    let mut root = root
        .child(auth_title)
        .child(api_key_card)
        .child(usage_card);

    // 计费模式放在请求用量下面
    if let Some(card) = billing_mode_card {
        root = root.child(card);
    }

    // 升级为付费版放在计费模式下面
    if let Some(card) = upgrade_card {
        root = root.child(card);
    }

    root = root
        .child(refresh_button)
        .child(search_title)
        .child(range_card)
        .child(number_card);

    for card in info_cards {
        root = root.child(card);
    }

    root.child(more_title)
        .child(help_card)
        .child(qq_card)
        .child(delete_auth_card)
        .child(build_title)
        .child(build_time_row)
        .child(build_user_row)
        .child(build_branch_row)
        .child(build_hash_row)
}

/// 构建APIKey状态卡片
fn build_apikey_status_card(state: &UiState) -> ui::Element {
    // 第一个卡片：APIKey状态 + 显示/隐藏
    let card1 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(8);

    // 第一行：APIKey状态 + 已验证/未验证
    let row1 = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8);

    let status_label = ui::Element::new(ui::ElementType::P, Some("APIKey状态"))
        .size(15)
        .flex_shrink(0.0);

    let spacer1 = ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0);

    let (status_text, status_color) = if state.api_key_verified {
        ("已验证", "#00FF00")
    } else {
        ("未验证", "#FF4444")
    };

    let status_value = ui::Element::new(ui::ElementType::P, Some(status_text))
        .size(15)
        .text_color(status_color)
        .flex_shrink(0.0);

    // 第二行：APIKey显示/隐藏（可点击）
    let row2 = if state.api_key_verified && !state.api_key.is_empty() {
        let api_key_display = if state.api_key_visible {
            state.api_key.clone()
        } else {
            "********************************".to_string()
        };

        let hint_text = if state.api_key_visible {
            "点击以隐藏APIKey,请勿传播/分享"
        } else {
            "点击以显示APIKey,请勿传播/分享"
        };

        let api_key_text = ui::Element::new(ui::ElementType::P, Some(&api_key_display))
            .size(15)
            .text_color("#888888");

        let hint_label = ui::Element::new(ui::ElementType::P, Some(hint_text))
            .size(15);

        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Column)
            .width_full()
            .on(ui::Event::Click, TOGGLE_APIKEY_VISIBLE_EVENT)
            .child(api_key_text)
            .child(hint_label)
    } else {
        ui::Element::new(ui::ElementType::Div, None)
    };

    card1.child(row1.child(status_label).child(spacer1).child(status_value))
        .child(row2)
}

/// 构建请求用量卡片
fn build_usage_card(state: &UiState) -> ui::Element {
    let container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg("#1E1E1F")
        .radius(12)
        .padding(12)
        .gap(8);

    // 如果有请求用量数据
    if let Some(ref info) = state.server_device_info {
        // API返回的是 {"status":200, "result": {...}}，需要先提取result
        let result = info.get("result").unwrap_or(info);

        if let Some(all_req_str) = result.get("ALLRequests").and_then(|v| v.as_str()) {
            let used_req = result.get("UseRequests").and_then(|v| v.as_str()).unwrap_or("0");
            let all_req: f64 = all_req_str.parse().unwrap_or(0.0);
            let used: f64 = used_req.parse().unwrap_or(0.0);

            let usage_text = format!("{} / {}", used_req, all_req_str);

            // 第一行：请求用量 + 数值
            let row1 = ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .align_center()
                .width_full()
                .gap(8);

            let usage_label = ui::Element::new(ui::ElementType::P, Some("请求用量："))
                .size(15)
                .flex_shrink(0.0);

            let spacer = ui::Element::new(ui::ElementType::Div, None)
                .flex_grow(1.0);

            let usage_value = ui::Element::new(ui::ElementType::P, Some(&usage_text))
                .size(15)
                .text_color("#BBBBBB")
                .flex_shrink(0.0);

            // 进度条
            let progress_percent = if all_req > 0.0 {
                ((used / all_req * 100.0).min(100.0) as u32).to_string()
            } else {
                "0".to_string()
            };

            let progress_bar = ui::Element::new(ui::ElementType::Progress, None)
                .width_full()
                .height(15)
                .prop("value", &progress_percent);

            return container
                .child(row1.child(usage_label).child(spacer).child(usage_value))
                .child(progress_bar);
        }
    }

    // 没有数据时显示提示
    let hint = ui::Element::new(ui::ElementType::P, Some("请先验证设备获取用量信息"))
        .size(13)
        .text_color("#888888");

    container.child(hint)
}

// ========== 辅助函数 ==========

fn build_switch(is_on: bool, event_id: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Switch, None)
        .on(ui::Event::Change, event_id)
        .prop("checked", if is_on { "true" } else { "false" })
}

fn build_settings_card(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
) -> ui::Element {
    build_settings_card_colored(icon_svg, title, desc, right, click_event, "#1E1E1F", "#FFFFFF")
}

/// 构建带自定义颜色的设置卡片
fn build_settings_card_colored(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
    bg_color: &str,
    text_color: &str,
) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22)
        .text_color(text_color);

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

    let title_el = ui::Element::new(ui::ElementType::P, Some(title))
        .size(15)
        .text_color(text_color);
    text_col = text_col.child(title_el);

    if let Some(desc_text) = desc {
        let desc_el = ui::Element::new(ui::ElementType::P, Some(desc_text))
            .size(13)
            .text_color(text_color);
        text_col = text_col.child(desc_el);
    }

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg(bg_color)
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

fn build_icon_text_button_full(label: &str, icon_svg: String, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22);

    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(12)
        .padding(14)
        .bg("#2A2A2A")
        .width_full()
        .flex()
        .align_center()
        .gap(8)
        .child(icon)
        .child(text)
}

fn build_days_card(state: &UiState) -> ui::Element {
    let selected_text = format!("{}天", state.selected_days);
    let options = [7u32, 14, 30];

    let mut select = ui::Element::new(ui::ElementType::Select, Some(&selected_text))
        .on(ui::Event::Change, DAYS_DROPDOWN_EVENT)
        .radius(8)
        .padding_left(12)
        .padding_right(12)
        .bg("#2A2A2A")
        .size(14);

    for day in options.iter() {
        let option_text = format!("{}天", day);
        let option = ui::Element::new(ui::ElementType::Option, Some(&option_text));
        select = select.child(option);
    }

    build_settings_card(
        icons::calendar_svg(),
        "同步天气天数",
        None,
        Some(select),
        None,
    )
}

fn build_sync_card_text() -> String {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let location = if state.last_sync_location.is_empty() {
        "暂无"
    } else {
        state.last_sync_location.as_str()
    };

    let (time_text, expired) = if state.last_sync_time_ms == 0 {
        ("暂无".to_string(), false)
    } else {
        let elapsed = now_ms().saturating_sub(state.last_sync_time_ms);
        let expire_ms = state.selected_days as u64 * 24 * 60 * 60 * 1000;
        let expired = expire_ms > 0 && elapsed > expire_ms;
        (format_relative(elapsed), expired)
    };

    let expired_mark = if expired { " (已过期)" } else { "" };
    format!(
        "地区: {}\n时间: {}{}",
        location, time_text, expired_mark
    )
}

fn format_relative(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1000;
    if seconds < 60 {
        return "刚刚".to_string();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{}分钟前", minutes);
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{}小时前", hours);
    }
    let days = hours / 24;
    format!("{}天前", days)
}

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
            if is_leap_year(y) { 29 } else { 28 }
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

pub fn rerender_main_ui() {
    let element_id = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id.clone()
    };

    if let Some(element_id) = element_id {
        let ui_tree = build_main_ui();
        psys_host::ui_v3::render(&element_id, ui_tree);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}