use super::state::*;
use crate::astrobox::psys_host;
use crate::astrobox::psys_host::dialog;
use crate::astrobox::psys_host::interconnect;
use crate::astrobox::psys_host::register;
use crate::astrobox::psys_host::thirdpartyapp;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use urlencoding::encode;

// ========== 事件ID常量 ==========

pub const SEND_BUTTON_EVENT: &str = "send_button";
pub const TAB_SYNC_EVENT: &str = "tab_sync";
pub const TAB_CITY_EVENT: &str = "tab_city";
pub const TAB_NOTICE_EVENT: &str = "tab_notice";
pub const TAB_SETTINGS_EVENT: &str = "tab_settings";
pub const ALERTS_SYNC_TOGGLE_EVENT: &str = "alerts_sync_toggle";
pub const OPEN_HELP_DOC_EVENT: &str = "open_help_doc";
pub const OPEN_QQ_GROUP_EVENT: &str = "open_qq_group";
pub const DAYS_DROPDOWN_EVENT: &str = "days_dropdown";
pub const GET_CITYLIST_EVENT: &str = "get_citylist";
pub const SELECT_CITY_DROPDOWN_EVENT: &str = "select_city_dropdown";
pub const DELETE_CITY_PREFIX: &str = "delete_city:";
pub const CHECK_PAYMENT_EVENT: &str = "check_payment";
pub const UPGRADE_TO_PAID_EVENT: &str = "upgrade_to_paid";
pub const OPEN_PAY_URL_EVENT: &str = "open_pay_url";
pub const REFRESH_DEVICE_INFO_EVENT: &str = "refresh_device_info";
pub const FREE_VERSION_EVENT: &str = "free_version";
pub const OPEN_VERIFY_URL_EVENT: &str = "open_verify_url";
pub const CITY_ORDER_PREFIX: &str = "city_order:";
pub const TOGGLE_APIKEY_VISIBLE_EVENT: &str = "toggle_apikey_visible";
pub const SEARCH_CITY_EVENT: &str = "search_city";
pub const ADD_CITY_PREFIX: &str = "add_city:";
pub const SEARCH_CITY_BUTTON_EVENT: &str = "search_city_button";
pub const SEARCH_RANGE_EVENT: &str = "search_range";
pub const SEARCH_NUMBER_EVENT: &str = "search_number";
pub const TOGGLE_SEARCH_RESULTS_EVENT: &str = "toggle_search_results";
pub const REFRESH_NOTICE_EVENT: &str = "refresh_notice";
pub const OPEN_NOTICE_LINK_PREFIX: &str = "open_notice_link:";

pub const DELETE_LOCAL_AUTH_EVENT: &str = "delete_local_auth";

// ========== Interconnect消息处理 ==========

/// 处理来自快应用的消息
pub fn handle_interconnect_message(payload: &str) {
    tracing::info!("收到快应用消息: {}", payload);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        // 从 payloadText 字段提取实际消息内容
        let msg_json = if let Some(payload_text) = json.get("payloadText").and_then(|v| v.as_str()) {
            match serde_json::from_str::<serde_json::Value>(payload_text) {
                Ok(inner) => inner,
                Err(e) => {
                    tracing::error!("解析 payloadText 失败: {}", e);
                    return;
                }
            }
        } else {
            json
        };

        let msg_type = msg_json.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let status = msg_json.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let data = msg_json.get("data");

        tracing::info!("解析消息: type={}, status={}", msg_type, status);

        match msg_type {
            "APIKEY" => {
                if status == "OK" {
                    if let Some(api_key) = data.and_then(|v| v.as_str()) {
                        handle_apikey_received(api_key);
                    }
                } else {
                    handle_apikey_invalid();
                }
            }
            "DEVICEINFO" => {
                if status == "OK" {
                    if let Some(device_info_json) = data {
                        handle_device_info_received(device_info_json);
                    }
                }
            }
            "CITYLIST" => {
                if status == "OK" {
                    if let Some(cities) = data.and_then(|v| v.as_array()) {
                        handle_citylist_received(cities);
                    }
                }
            }
            "PUT_CITY_DONE" => {
                if status == "OK" {
                    show_alert("成功", "城市添加成功");
                    request_citylist_from_device();
                } else {
                    show_alert("失败", "城市添加失败");
                }
            }
            "DEL_CITY_DONE" => {
                if status == "OK" {
                    show_alert("成功", "城市删除成功");
                    request_citylist_from_device();
                } else {
                    show_alert("失败", "城市删除失败");
                }
            }
            "PUT_SETTINGS_DONE" => {
                if status == "OK" {
                    tracing::info!("设置保存成功");
                }
            }
            "PUT_WEATHERDATA_DONE" => {
                if status == "OK" {
                    tracing::info!("天气数据同步成功");
                } else {
                    show_alert("失败", &format!("同步失败: {}", status));
                }
            }
            "ORDER_CITY_DONE" => {
                if status == "OK" {
                    tracing::info!("城市排序成功");
                    request_citylist_from_device();
                }
            }
            _ => {
                tracing::info!("未处理的消息类型: {}", msg_type);
            }
        }
    }
}

pub fn handle_timer_payload(payload: &str) {
    tracing::info!("timer payload: {}", payload);
}

pub fn ui_event_processor(
    event_type: crate::exports::astrobox::psys_plugin::event_v3::Event,
    event_id: &str,
    event_payload: &str,
) {
    tracing::info!("UI Event: type={:?}, id={}", event_type, event_id);

    match event_id {
        SEND_BUTTON_EVENT => send_weather_data(),
        TAB_SYNC_EVENT => switch_tab(MainTab::SyncData),
        TAB_CITY_EVENT => switch_tab(MainTab::CityManage),
        TAB_NOTICE_EVENT => switch_tab(MainTab::Notice),
        TAB_SETTINGS_EVENT => switch_tab(MainTab::Settings),
        OPEN_HELP_DOC_EVENT => open_help_doc_page(),
        OPEN_QQ_GROUP_EVENT => open_qq_group_page(),
        ALERTS_SYNC_TOGGLE_EVENT => toggle_alerts_sync(),
        REFRESH_NOTICE_EVENT => fetch_notice_list(),
        DAYS_DROPDOWN_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            if let Some(day_str) = parsed_value.strip_suffix('天') {
                if let Ok(day) = day_str.trim().parse::<u32>() {
                    select_days(day);
                }
            }
        }
        GET_CITYLIST_EVENT => {
            // 检查是否已经在加载中
            let is_loading = {
                let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.city_list_loading
            };

            if is_loading {
                tracing::info!("城市列表正在加载中，忽略重复请求");
                return;
            }

            tracing::info!("刷新城市列表");
            // 设置加载状态
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.city_list_loading = true;
            }
            crate::ui::build::rerender_main_ui();
            request_citylist_from_device();
        }
        TOGGLE_APIKEY_VISIBLE_EVENT => toggle_apikey_visible(),
        SEARCH_CITY_BUTTON_EVENT => {
            let keyword = {
                let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.city_search_keyword.clone()
            };
            search_city(&keyword);
        }
        SEARCH_RANGE_EVENT => {
            let value = parse_event_value(event_payload);
            tracing::info!("SEARCH_RANGE_EVENT value: {}", value);
            // value 是文本: "全球", "中国", "日本"
            let range = match value.as_str() {
                "中国" => "cn",
                "日本" => "jp",
                _ => "", // 全球或其他
            };
            tracing::info!("SEARCH_RANGE_EVENT resolved range: '{}'", range);
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.city_search_range = range.to_string();
            }
            let _ = crate::ui::state::save_all_settings();
            crate::ui::build::rerender_main_ui();
        }
        SEARCH_NUMBER_EVENT => {
            let value = parse_event_value(event_payload);
            tracing::info!("SEARCH_NUMBER_EVENT value: {}", value);
            // value 是文本: "5 个", "10 个" 等
            let num = value
                .trim()
                .trim_end_matches(" 个")
                .parse::<u32>()
                .unwrap_or(10);
            tracing::info!("SEARCH_NUMBER_EVENT resolved num: {}", num);
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.city_search_number = num;
            }
            let _ = crate::ui::state::save_all_settings();
            crate::ui::build::rerender_main_ui();
        }
        TOGGLE_SEARCH_RESULTS_EVENT => {
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.search_results_expanded = !state.search_results_expanded;
            }
            crate::ui::build::rerender_main_ui();
        }
        CHECK_PAYMENT_EVENT => check_payment_status(),
        UPGRADE_TO_PAID_EVENT => start_verification(false),
        OPEN_PAY_URL_EVENT => open_pay_url(),
        REFRESH_DEVICE_INFO_EVENT => refresh_device_info(),
        FREE_VERSION_EVENT => verify_free_version(),
        OPEN_VERIFY_URL_EVENT => open_verify_url_from_state(),
        DELETE_LOCAL_AUTH_EVENT => delete_local_auth(),
        SELECT_CITY_DROPDOWN_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            tracing::info!("SELECT_CITY_DROPDOWN_EVENT: payload={}, parsed={}", event_payload, parsed_value);
            // Select 返回选中项的文本，需要通过城市名匹配索引
            select_city_by_name(&parsed_value);
        }
        _ => {}
    }

    if event_id.starts_with(DELETE_CITY_PREFIX) {
        if let Some(idx_str) = event_id.strip_prefix(DELETE_CITY_PREFIX) {
            if let Ok(idx) = idx_str.parse::<usize>() {
                delete_city(idx);
            }
        }
    }

    if event_id.starts_with(ADD_CITY_PREFIX) {
        if let Some(idx_str) = event_id.strip_prefix(ADD_CITY_PREFIX) {
            if let Ok(idx) = idx_str.parse::<usize>() {
                add_city_to_device(idx);
            }
        }
    }

    // 公告链接点击
    if event_id.starts_with(OPEN_NOTICE_LINK_PREFIX) {
        if let Some(url) = event_id.strip_prefix(OPEN_NOTICE_LINK_PREFIX) {
            dialog::open_url(url);
        }
    }

    // 搜索输入框更新关键词
    if event_id == "city_search_input" {
        let keyword = parse_event_value(event_payload);
        // 忽略JSON格式的事件值
        if !keyword.starts_with("{") {
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.city_search_keyword = keyword;
        }
    }

    if event_id.starts_with(CITY_ORDER_PREFIX) {
        if let Some(params) = event_id.strip_prefix(CITY_ORDER_PREFIX) {
            if let Some((idx_str, offset_str)) = params.split_once(',') {
                if let (Ok(idx), Ok(offset)) = (idx_str.parse::<usize>(), offset_str.parse::<i32>()) {
                    order_city(idx, offset);
                }
            }
        }
    }
}

// ========== 辅助函数 ==========

fn parse_event_value(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        if let Some(value) = extract_event_value(&json) {
            return value.trim().to_string();
        }
    }
    payload.trim().to_string()
}

fn extract_event_value(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    for key in ["value", "text", "content", "label"] {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    for key in ["detail", "target", "currentTarget", "data"] {
        if let Some(nested) = value.get(key) {
            if let Some(text) = extract_event_value(nested) {
                return Some(text);
            }
        }
    }
    None
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn switch_tab(tab: MainTab) {
    let should_rerender = {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.current_tab != tab {
            state.current_tab = tab;
            true
        } else {
            false
        }
    };
    if should_rerender {
        crate::ui::build::rerender_main_ui();
    }
}

fn toggle_alerts_sync() {
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sync_alerts_enabled = !state.sync_alerts_enabled;
    }
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn select_days(day: u32) {
    if day == 0 { return; }
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.selected_days = day;
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

/// 根据城市名称选择城市（Select返回的是文本）
fn select_city_by_name(name: &str) {
    // 去掉可能的后缀 " · adm1"
    let city_name = name.split(" · ").next().unwrap_or(name).trim();

    // 先查找城市信息并克隆
    let found = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.city_list.iter().position(|c| c.name == city_name).map(|idx| {
            let city = &state.city_list[idx];
            (idx, city.name.clone(), city.adm1.clone(), city.adm2.clone(), city.lat.clone(), city.lon.clone())
        })
    };

    if let Some((idx, name, adm1, adm2, lat, lon)) = found {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.selected_city_index = Some(idx);
        state.selected_location_id = name.clone();
        state.selected_location_name = name;
        state.selected_location_adm1 = adm1;
        state.selected_location_adm2 = adm2;
        state.selected_location_lat = lat;
        state.selected_location_lon = lon;
        drop(state);
        let _ = crate::ui::state::save_all_settings();
        crate::ui::build::rerender_main_ui();
    }
}

fn select_sync_city(idx: usize) {
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    if idx < state.city_list.len() {
        let city = &state.city_list[idx];
        let name = city.name.clone();
        let adm1 = city.adm1.clone();
        let adm2 = city.adm2.clone();
        let lat = city.lat.clone();
        let lon = city.lon.clone();

        state.selected_city_index = Some(idx);
        state.selected_location_id = name.clone();
        state.selected_location_name = name;
        state.selected_location_adm1 = adm1;
        state.selected_location_adm2 = adm2;
        state.selected_location_lat = lat;
        state.selected_location_lon = lon;
    }
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

// ========== 验证流程 ==========

fn handle_apikey_received(api_key: &str) {
    tracing::info!("收到APIKey: {}", api_key);

    if api_key.trim().is_empty() {
        handle_apikey_invalid();
        return;
    }

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key = api_key.to_string();
        state.api_key_verified = true;
        state.verification_status = VerificationStatus::Verified;
    }

    let _ = crate::ui::state::save_all_settings();
    show_alert("成功", "APIKey验证成功");
    crate::ui::build::rerender_main_ui();

    // 获取设备信息（请求用量等）
    wit_bindgen::block_on(async move {
        if get_device_addr().await.is_some() {
            fetch_device_info_from_server();
        }
    });
}

fn handle_apikey_invalid() {
    tracing::info!("APIKey无效，需要验证");
    wit_bindgen::block_on(async move {
        if let Some(device_addr) = get_device_addr().await {
            get_device_info_and_verify(&device_addr, false);
        }
    });
}

fn handle_device_info_received(data: &serde_json::Value) {
    tracing::info!("收到设备信息: {:?}", data);

    let device_info = parse_device_info(data);

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.device_info = Some(device_info);
        state.verification_status = VerificationStatus::WaitingPayment;
    }

    // 不自动跳转，让用户自己点击
    crate::ui::build::rerender_main_ui();
}

fn parse_device_info(data: &serde_json::Value) -> DeviceInfo {
    DeviceInfo {
        // 验证需要的字段
        product: data.get("product").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        deviceId: data.get("deviceId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        serial: data.get("serial").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        // 屏幕信息
        screenWidth: data.get("screenWidth").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        screenHeight: data.get("screenHeight").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        screenShape: data.get("screenShape").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        screenDensity: data.get("screenDensity").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        // 设备基本信息
        deviceType: data.get("deviceType").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        brand: data.get("brand").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        model: data.get("model").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        // 蓝牙地址
        btAddr: data.get("btAddr").or_else(|| data.get("bt_address")).or_else(|| data.get("mac")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
    }
}

/// 从状态中获取设备信息并打开验证页面
fn open_verify_url_from_state() {
    let device_info = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.device_info.clone()
    };

    if let Some(device_info) = device_info {
        let timestamp = now_ms() / 1000;
        // 拼接格式: product.deviceId.serial.timestamp
        let verify_data = format!(
            "{}.{}.{}.{}",
            device_info.product,
            device_info.deviceId,
            device_info.serial,
            timestamp
        );

        let encoded_data = encode(&verify_data);
        let verify_url = format!(
            "{}/api/v2/verify/Eternal?data={}",
            server_api_base(),
            encoded_data
        );

        tracing::info!("打开验证页面: {}", verify_url);
        dialog::open_url(&verify_url);
    } else {
        show_alert("错误", "设备信息缺失");
    }
}

fn open_verification_url(device_info: &DeviceInfo) {
    let timestamp = now_ms() / 1000;
    // 拼接格式: product.deviceId.serial.timestamp
    let verify_data = format!(
        "{}.{}.{}.{}",
        device_info.product,
        device_info.deviceId,
        device_info.serial,
        timestamp
    );

    let encoded_data = encode(&verify_data);
    let verify_url = format!(
        "{}/api/v2/verify/Eternal?data={}",
        server_api_base(),
        encoded_data
    );

    tracing::info!("打开验证页面: {}", verify_url);
    dialog::open_url(&verify_url);
}

/// 免费版验证
fn verify_free_version() {
    tracing::info!("免费版验证...");

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.verification_status = VerificationStatus::VerifyingPayment;
    }
    crate::ui::build::rerender_main_ui();

    wit_bindgen::block_on(async move {
        let device_info = {
            let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.device_info.clone()
        };

        if let Some(device_info) = device_info {
            let timestamp = now_ms() / 1000;
            // 拼接格式: product.deviceId.serial.timestamp
            let verify_data = format!(
                "{}.{}.{}.{}",
                device_info.product,
                device_info.deviceId,
                device_info.serial,
                timestamp
            );

            let encoded_data = encode(&verify_data);
            let check_url = format!(
                "{}/api/v2/verifyCheck/Eternal?data={}&type=free",
                server_api_base(),
                encoded_data
            );

            tracing::info!("免费版验证URL: {}", check_url);

            // 使用不需要认证的请求
            match super::api_client::get_json_no_auth(&check_url) {
                Ok(json) => {
                    tracing::info!("verifyCheck free response: {:?}", json);
                    if json.get("status").and_then(|v| v.as_i64()) == Some(200) {
                        if let Some(result) = json.get("result") {
                            let api_key = result.get("APIKey").and_then(|v| v.as_str()).unwrap_or("");
                            let signature = result.get("signature").and_then(|v| v.as_str()).unwrap_or("");

                            tracing::info!("APIKey: {}, signature length: {}", api_key, signature.len());

                            if verify_api_key_signature(api_key, signature) {
                                tracing::info!("签名验证成功，发送到设备");
                                send_put_settings(api_key);
                            } else {
                                tracing::error!("签名验证失败");
                                show_alert("错误", "签名验证失败");
                                set_verification_failed();
                            }
                        } else {
                            show_alert("错误", "返回数据格式错误");
                            set_verification_failed();
                        }
                    } else {
                        let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("未知错误");
                        show_alert("提示", &format!("验证失败: {}", msg));
                        set_verification_failed();
                    }
                }
                Err(e) => {
                    tracing::error!("verify_free_version error: {}", e);
                    show_alert("失败", &format!("请求失败: {}", e));
                    set_verification_failed();
                }
            }
        } else {
            show_alert("错误", "设备信息缺失，请重新验证");
            set_verification_failed();
        }
    });
}

fn check_payment_status() {
    tracing::info!("检查付款状态...");

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.verification_status = VerificationStatus::VerifyingPayment;
    }
    crate::ui::build::rerender_main_ui();

    wit_bindgen::block_on(async move {
        let device_info = {
            let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.device_info.clone()
        };

        if let Some(device_info) = device_info {
            let timestamp = now_ms() / 1000;
            // 拼接格式: product.deviceId.serial.timestamp
            let verify_data = format!(
                "{}.{}.{}.{}",
                device_info.product,
                device_info.deviceId,
                device_info.serial,
                timestamp
            );

            let encoded_data = encode(&verify_data);
            let check_url = format!(
                "{}/api/v2/verifyCheck/Eternal?data={}&type=normal",
                server_api_base(),
                encoded_data
            );

            // 使用不需要认证的请求
            match super::api_client::get_json_no_auth(&check_url) {
                Ok(json) => {
                    tracing::info!("verifyCheck response: {:?}", json);
                    if json.get("status").and_then(|v| v.as_i64()) == Some(200) {
                        if let Some(result) = json.get("result") {
                            let api_key = result.get("APIKey").and_then(|v| v.as_str()).unwrap_or("");
                            let signature = result.get("signature").and_then(|v| v.as_str()).unwrap_or("");

                            if verify_api_key_signature(api_key, signature) {
                                send_put_settings(api_key);
                            } else {
                                show_alert("错误", "签名验证失败");
                                set_verification_failed();
                            }
                        }
                    } else {
                        show_alert("提示", "请先完成付款");
                        set_verification_failed();
                    }
                }
                Err(e) => {
                    tracing::error!("check_payment_status error: {}", e);
                    show_alert("失败", &format!("检查失败: {}", e));
                    set_verification_failed();
                }
            }
        } else {
            show_alert("错误", "设备信息缺失，请重新验证");
            set_verification_failed();
        }
    });
}

/// 验证APIKey（跳过签名验证，直接写入）
fn verify_api_key_signature(_api_key: &str, _signature: &str) -> bool {
    // 直接返回 true，跳过签名验证
    tracing::info!("跳过签名验证，直接写入APIKey");
    true
}

fn send_put_settings(api_key: &str) {
    tracing::info!("发送PUT_SETTINGS: {}", api_key);

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key = api_key.to_string();
        state.api_key_verified = true;
        state.verification_status = VerificationStatus::Verified;
    }
    let _ = crate::ui::state::save_all_settings();

    wit_bindgen::block_on(async move {
        if let Some(device_addr) = get_device_addr().await {
            let payload = serde_json::json!({
                "type": "PUT_SETTINGS",
                "data": { "APIKey": api_key }
            }).to_string();
            send_interconnect_message(&device_addr, &payload).await;
        }
    });

    show_alert("成功", "验证成功！");
    crate::ui::build::rerender_main_ui();

    // 刷新设备信息
    fetch_device_info_from_server();
}

fn set_verification_failed() {
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.verification_status = VerificationStatus::Failed;
    }
    crate::ui::build::rerender_main_ui();
}

fn start_verification(_is_free: bool) {
    tracing::info!("开始验证流程");

    // 检查是否已有APIKey
    let existing_api_key = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key.clone()
    };

    if !existing_api_key.is_empty() {
        // 已有APIKey，直接刷新设备信息
        tracing::info!("已有APIKey，刷新设备信息");
        fetch_device_info_from_server();
        return;
    }

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.verification_status = VerificationStatus::CheckingDevice;
    }
    crate::ui::build::rerender_main_ui();

    wit_bindgen::block_on(async move {
        let device_addr = match get_device_addr().await {
            Some(addr) => addr,
            None => {
                show_alert("错误", "没有连接的设备");
                set_verification_failed();
                return;
            }
        };

        // get_device_addr 已存储 host_device_info，重新渲染以显示设备信息
        crate::ui::build::rerender_main_ui();

        // 先请求APIKey
        request_apikey_from_device(&device_addr);
    });
}

/// 从设备请求APIKey
fn request_apikey_from_device(device_addr: &str) {
    tracing::info!("请求APIKey...");

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.verification_status = VerificationStatus::GettingAPIKey;
    }
    crate::ui::build::rerender_main_ui();

    let payload = serde_json::json!({ "type": "GET_APIKEY" }).to_string();

    wit_bindgen::block_on(async move {
        // 注册接收
        let _ = register::register_interconnect_recv(device_addr, QA_PKG_NAME).await;

        if send_interconnect_message(device_addr, &payload).await {
            tracing::info!("GET_APIKEY 已发送，等待响应...");
        }
    });
}

fn get_device_info_and_verify(device_addr: &str, _is_free: bool) {
    tracing::info!("获取设备信息...");

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.verification_status = VerificationStatus::GettingDeviceInfo;
    }
    crate::ui::build::rerender_main_ui();

    let payload = serde_json::json!({ "type": "GET_DEVICEINFO" }).to_string();

    wit_bindgen::block_on(async move {
        if send_interconnect_message(device_addr, &payload).await {
            tracing::info!("GET_DEVICEINFO 已发送，等待响应...");
        }
    });
}

fn refresh_device_info() {
    tracing::info!("刷新设备信息...");
    fetch_device_info_from_server();
}

fn toggle_apikey_visible() {
    tracing::info!("切换APIKey显示/隐藏");
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key_visible = !state.api_key_visible;
    }
    crate::ui::build::rerender_main_ui();
}

pub fn fetch_device_info_from_server() {
    let api_key = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key.clone()
    };

    if api_key.is_empty() {
        return;
    }

    wit_bindgen::block_on(async move {
        let url = format!("{}/api/v2/getInfo/Eternal", server_api_base());
        let body = serde_json::json!({ "Key": api_key });

        // 使用带状态码的请求，以便区分 200（正常）和 201（授权失效）
        match super::api_client::post_json_no_auth_with_status(&url, &body) {
            Ok((status, json)) => {
                if status == 201 {
                    // 设备用量信息无法获取，授权可能已过期
                    tracing::warn!("getInfo returned 201, authorization may be expired");
                    handle_reactivation_needed();
                    return;
                }

                // status == 200，正常处理
                tracing::info!("getInfo response: {:?}", json);
                {
                    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.server_device_info = Some(json);
                }
                // 在设置完状态后立即刷新UI
                crate::ui::build::rerender_main_ui();
                // 显示成功通知
                show_alert("成功", "授权信息已刷新");
            }
            Err(e) => {
                tracing::error!("获取设备信息失败: {}", e);
                show_alert("失败", &format!("刷新失败: {}", e));
            }
        }
    });
}

/// 设备授权失效（HTTP 201），清空APIKey并提示用户重新激活
fn handle_reactivation_needed() {
    // 清空APIKey和验证状态，重置为未激活状态
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key.clear();
        state.api_key_verified = false;
        state.verification_status = VerificationStatus::NotStarted;
        state.server_device_info = None;
    }

    // 保存设置（将清空后的APIKey写入磁盘，避免重启后仍使用失效的key）
    let _ = crate::ui::state::save_all_settings();

    // 重新渲染UI，显示激活页面
    crate::ui::build::rerender_main_ui();

    // 弹出对话框提示用户重新激活
    show_alert("授权失效", "设备用量信息无法获取，您的授权可能已过期，请重新激活");
}

// ========== 天气同步 ==========

fn send_weather_data() {
    let (api_key, selected_idx, city_list, selected_days, sync_alerts) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.api_key.clone(),
            state.selected_city_index,
            state.city_list.clone(),
            state.selected_days,
            state.sync_alerts_enabled,
        )
    };

    if api_key.is_empty() {
        show_alert("提示", "请先验证设备");
        return;
    }

    let city = match selected_idx {
        Some(idx) if idx < city_list.len() => &city_list[idx],
        _ => {
            show_alert("提示", "请先选择城市");
            return;
        }
    };

    let city_index = selected_idx.unwrap_or(0);
    let city_clone = city.clone();
    let api_key_clone = api_key.clone();
    let sync_alerts_clone = sync_alerts;
    let days_to_sync = selected_days;

    // 初始化同步进度（从0开始，获取数据后才显示实际进度）
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sync_progress = SyncProgress {
            syncing: true,
            current_day: 0,
            total_days: days_to_sync,
            status_text: "获取天气数据...".to_string(),
        };
    }
    crate::ui::build::rerender_main_ui();

    mark_sync_started(&city_clone);

    wit_bindgen::block_on(async move {
        let mut error_msg = String::new();

        // 向API请求用户选择的天数
        let url = format!("{}/api/v2/3f/getWeather/Eternal", server_api_base());
        let body = serde_json::json!({
            "Key": &api_key_clone,
            "longitude": &city_clone.lon,
            "latitude": &city_clone.lat,
            "days": days_to_sync
        });

        match super::api_client::post_json_no_auth(&url, &body) {
            Ok(weather_json) => {
                // 检查设备连接
                let device_addr = match get_device_addr().await {
                    Some(addr) => addr,
                    None => {
                        error_msg = "设备连接丢失".to_string();
                        {
                            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                            state.sync_progress.syncing = false;
                        }
                        crate::ui::build::rerender_main_ui();
                        show_alert("失败", &error_msg);
                        return;
                    }
                };

                // API返回的数据结构: {"status":200,"result":[...天气数据...]}
                // result字段是数组，包含每天的天气数据
                let daily = weather_json.get("result").and_then(|v| v.as_array()).cloned().unwrap_or_default();

                if daily.is_empty() {
                    error_msg = "未获取到天气数据".to_string();
                } else {
                    // 每14天为一块，倒序发送
                    let chunk_size = 14;
                    let chunks: Vec<_> = daily.chunks(chunk_size).collect();
                    let total_chunks = chunks.len() as u32;

                    for (chunk_idx, chunk) in chunks.into_iter().enumerate().rev() {
                        let chunk_num = total_chunks - chunk_idx as u32;

                        // 更新进度
                        {
                            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                            state.sync_progress.current_day = chunk_num;
                            state.sync_progress.total_days = total_chunks;
                            state.sync_progress.status_text = format!("发送数据块 {}/{}", chunk_num, total_chunks);
                        }
                        crate::ui::build::rerender_main_ui();

                        // 构建该块的天气数据
                        let mut chunk_json = weather_json.clone();
                        chunk_json["result"] = serde_json::Value::Array(chunk.to_vec());

                        let payload = serde_json::json!({
                            "type": "PUT_WEATHERDATA",
                            "data": {
                                "cityindex": city_index,
                                "result": chunk_json
                            }
                        }).to_string();

                        send_interconnect_message(&device_addr, &payload).await;
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
            Err(e) => {
                error_msg = format!("获取天气数据失败: {}", e);
            }
        }

        // 发送预警数据（如果开启）
        if sync_alerts_clone && error_msg.is_empty() {
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.sync_progress.status_text = "正在同步预警数据...".to_string();
            }
            crate::ui::build::rerender_main_ui();

            if let Err(e) = send_weather_alerts(&api_key_clone, &city_clone, city_index).await {
                tracing::warn!("预警数据同步失败: {}", e);
            }
        }

        // 完成同步
        {
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.sync_progress.syncing = false;
            state.sync_progress.status_text.clear();
        }
        crate::ui::build::rerender_main_ui();

        if error_msg.is_empty() {
            show_alert("成功", &format!("同步完成，共 {} 天", days_to_sync));
        } else {
            show_alert("失败", &error_msg);
        }
    });
}

/// 同步天气预警数据
async fn send_weather_alerts(api_key: &str, city: &CityInfo, city_index: usize) -> Result<(), String> {
    let url = format!("{}/api/v2/3f/getWarn/Eternal", server_api_base());
    let body = serde_json::json!({
        "Key": api_key,
        "longitude": city.lon,
        "latitude": city.lat
    });

    let json = super::api_client::post_json_no_auth(&url, &body)
        .map_err(|e| format!("获取预警数据失败: {}", e))?;

    let payload = serde_json::json!({
        "type": "PUT_WARNDATA",
        "data": {
            "cityindex": city_index,
            "result": json
        }
    }).to_string();

    if let Some(device_addr) = get_device_addr().await {
        send_interconnect_message(&device_addr, &payload).await;
    }

    Ok(())
}

fn mark_sync_started(city: &CityInfo) {
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_sync_time_ms = now_ms();
    state.last_sync_location = city.name.clone();
    drop(state);
    crate::ui::render_sync_card(crate::ui::SYNC_CARD_ID);
}

// ========== 设备通信 ==========

async fn get_device_addr() -> Option<String> {
    let devices = psys_host::device::get_connected_device_list().await;
    tracing::info!("get_connected_device_list returned {} devices", devices.len());
    if let Some(device) = devices.first() {
        // 顺便存储设备信息，供激活页面显示使用
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.host_device_info = Some((device.name.clone(), device.addr.clone()));
        Some(device.addr.clone())
    } else {
        None
    }
}

async fn send_interconnect_message(device_addr: &str, payload: &str) -> bool {
    tracing::info!("发送Interconnect消息: device={}, payload={}", device_addr, payload);

    if !ensure_app_launched(device_addr).await {
        return false;
    }

    let _ = register::register_interconnect_recv(device_addr, QA_PKG_NAME).await;

    match interconnect::send_qaic_message(device_addr, QA_PKG_NAME, payload).await {
        Ok(_) => {
            tracing::info!("消息发送成功");
            true
        }
        Err(e) => {
            tracing::error!("消息发送失败: {:?}", e);
            false
        }
    }
}

async fn send_weather_to_device(payload: &str) -> Result<(), String> {
    let device_addr = get_device_addr().await.ok_or("没有连接的设备")?;
    send_interconnect_message(&device_addr, payload).await;
    std::thread::sleep(Duration::from_secs(2));
    Ok(())
}

async fn ensure_app_launched(device_addr: &str) -> bool {
    match thirdpartyapp::get_thirdparty_app_list(device_addr).await {
        Ok(app_list) => {
            let app = app_list.iter().find(|a| a.package_name == QA_PKG_NAME);
            if app.is_none() {
                show_alert("未安装", "请先安装永昼天气快应用");
                return false;
            }
            if let Some(app) = app {
                match thirdpartyapp::launch_qa(device_addr, app, "/index").await {
                    Ok(_) => {
                        tracing::info!("应用已启动");
                        std::thread::sleep(Duration::from_secs(2));
                        true
                    }
                    Err(e) => {
                        tracing::error!("启动应用失败: {:?}", e);
                        false
                    }
                }
            } else {
                false
            }
        }
        Err(e) => {
            tracing::error!("获取应用列表失败: {:?}", e);
            false
        }
    }
}

// ========== 城市管理 ==========

fn request_citylist_from_device() {
    tracing::info!("请求城市列表...");
    wit_bindgen::block_on(async move {
        match get_device_addr().await {
            Some(device_addr) => {
                let payload = serde_json::json!({ "type": "GET_CITYLIST" }).to_string();
                send_interconnect_message(&device_addr, &payload).await;
            }
            None => {
                // 没有连接设备，重置加载状态并提示
                {
                    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.city_list_loading = false;
                }
                show_alert("提示", "没有连接的设备");
                crate::ui::build::rerender_main_ui();
            }
        }
    });
}

fn handle_citylist_received(cities: &[serde_json::Value]) {
    tracing::info!("收到城市列表: {} 个城市", cities.len());

    let city_list: Vec<CityInfo> = cities
        .iter()
        .filter_map(|c| {
            Some(CityInfo {
                name: c.get("name").and_then(|v| v.as_str())?.to_string(),
                lat: c.get("lat").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                lon: c.get("lon").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                adm1: c.get("adm1").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                adm2: c.get("adm2").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                country: c.get("country").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        })
        .collect();

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.city_list = city_list;
        state.city_list_loading = false; // 重置加载状态
    }

    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn delete_city(idx: usize) {
    wit_bindgen::block_on(async move {
        if let Some(device_addr) = get_device_addr().await {
            let payload = serde_json::json!({
                "type": "DEL_CITY",
                "data": { "cityindex": idx }
            }).to_string();
            send_interconnect_message(&device_addr, &payload).await;
        }
    });
}

fn order_city(idx: usize, offset: i32) {
    show_alert("提示", "正在排序城市...");

    wit_bindgen::block_on(async move {
        if let Some(device_addr) = get_device_addr().await {
            let payload = serde_json::json!({
                "type": "ORDER_CITY",
                "data": { "cityindex": idx, "offset": offset }
            }).to_string();
            send_interconnect_message(&device_addr, &payload).await;
        }
    });
}

/// 搜索城市
fn search_city(keyword: &str) {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        // 清空搜索结果
        {
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.city_search_results.clear();
        }
        crate::ui::build::rerender_main_ui();
        return;
    }

    tracing::info!("搜索城市: {}", keyword);

    // 设置加载状态
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.city_search_keyword = keyword.to_string();
        state.city_search_loading = true;
    }
    crate::ui::build::rerender_main_ui();

    // 获取搜索设置
    let (search_range, search_number) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.city_search_range.clone(), state.city_search_number)
    };

    wit_bindgen::block_on(async move {
        let url = format!("{}/api/v2/3f/getCity/Eternal", server_api_base());
        let body = serde_json::json!({
            "Key": ui_state().read().unwrap().api_key,
            "location": keyword,
            "range": search_range,
            "number": search_number
        });

        tracing::info!("getCity request: url={}, body={}", url, serde_json::to_string(&body).unwrap_or_default());

        match super::api_client::post_json_no_auth(&url, &body) {
            Ok(json) => {
                tracing::info!("getCity response: {:?}", json);
                let result = json.get("result").unwrap_or(&json);
                let cities: Vec<CityInfo> = if let Some(arr) = result.as_array() {
                    arr.iter().filter_map(|c| {
                        Some(CityInfo {
                            name: c.get("name").and_then(|v| v.as_str())?.to_string(),
                            lat: c.get("lat").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            lon: c.get("lon").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            adm1: c.get("adm1").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            adm2: c.get("adm2").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            country: c.get("country").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        })
                    }).collect()
                } else {
                    Vec::new()
                };

                {
                    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.city_search_results = cities;
                    state.city_search_loading = false;
                }
            }
            Err(e) => {
                tracing::error!("搜索城市失败: {}", e);
                {
                    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.city_search_loading = false;
                }
            }
        }
        crate::ui::build::rerender_main_ui();
    });
}

/// 添加城市到设备
fn add_city_to_device(idx: usize) {
    let city = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.city_search_results.get(idx).cloned()
    };

    if let Some(city) = city {
        tracing::info!("添加城市: {:?}", city);

        wit_bindgen::block_on(async move {
            if let Some(device_addr) = get_device_addr().await {
                let payload = serde_json::json!({
                    "type": "PUT_CITY",
                    "data": {
                        "result": {
                            "name": city.name,
                            "lat": city.lat,
                            "lon": city.lon,
                            "adm1": city.adm1,
                            "adm2": city.adm2,
                            "country": city.country
                        }
                    }
                }).to_string();
                send_interconnect_message(&device_addr, &payload).await;
            }
        });

        // 清空搜索结果
        {
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.city_search_results.clear();
            state.city_search_keyword.clear();
        }
        crate::ui::build::rerender_main_ui();
    }
}

// ========== 页面跳转 ==========

fn open_help_doc_page() {
    let url = "https://docs.b4qaq.cn/docs/eternal";
    tracing::info!("打开帮助文档: {}", url);
    dialog::open_url(url);
}

fn open_qq_group_page() {
    let url = "http://qm.qq.com/cgi-bin/qm/qr?_wv=1027&k=1vc4XKmAyGeJJTmXumfkaaxRcQl1hMaK&authKey=vcKUTZ914E0kdmjzUejxXxz4AlnckuE0rKJ8xDWvOvZWGkm3dIB%2BP4axUSHxo%2FXt&noverify=0&group_code=1076096725";
    tracing::info!("打开QQ群: {}", url);
    dialog::open_url(url);
}

fn show_alert(title: &str, message: &str) {
    tracing::info!("show_alert: title={}, message={}", title, message);
    let title_str = title.to_string();
    let message_str = message.to_string();

    wit_bindgen::block_on(async move {
        let _ = dialog::show_dialog(
            dialog::DialogType::Alert,
            dialog::DialogStyle::Website,
            &dialog::DialogInfo {
                title: title_str,
                content: message_str,
                buttons: vec![dialog::DialogButton {
                    id: "ok".to_string(),
                    primary: true,
                    content: "确定".to_string(),
                }],
            },
        ).await;
    });
}

/// 打开支付页面（升级为付费版）
fn open_pay_url() {
    tracing::info!("打开支付页面");

    // 优先使用服务器返回的设备信息
    let server_info = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.server_device_info.clone()
    };

    if let Some(ref info) = server_info {
        let result = info.get("result").unwrap_or(info);
        // API返回: deviceID, onlyID
        let device_id = result.get("deviceID").and_then(|v| v.as_str()).unwrap_or("");
        let only_id = result.get("onlyID").and_then(|v| v.as_str()).unwrap_or("");

        if !device_id.is_empty() || !only_id.is_empty() {
            let timestamp = now_ms() / 1000;
            let verify_data = format!(
                "Eternal.{}.{}.{}",
                device_id, only_id, timestamp
            );
            let encoded_data = encode(&verify_data);
            let pay_url = format!(
                "{}/api/v2/verify/Eternal?data={}",
                server_api_base(),
                encoded_data
            );
            tracing::info!("打开支付页面: {}", pay_url);
            dialog::open_url(&pay_url);
            return;
        }
    }

    show_alert("提示", "请先验证设备");
}

/// 删除设备本地授权信息
fn delete_local_auth() {
    tracing::info!("删除本地授权信息...");

    // 清除所有本地存储的状态
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.api_key.clear();
        state.api_key_verified = false;
        state.api_key_visible = false;
        state.device_info = None;
        state.server_device_info = None;
        state.verification_status = VerificationStatus::NotStarted;
        state.city_list.clear();
        state.selected_city_index = None;
        state.city_search_keyword.clear();
        state.city_search_results.clear();
        state.last_sync_time_ms = 0;
        state.last_sync_location.clear();
        state.sync_progress = SyncProgress::default();
        state.notice_list.clear();
        state.settings_loaded = false; // 允许下次重新加载
    }

    // 删除本地设置文件
    let settings_file = "api_settings.json";
    if std::path::Path::new(settings_file).exists() {
        match std::fs::remove_file(settings_file) {
            Ok(()) => tracing::info!("已删除本地设置文件"),
            Err(e) => tracing::error!("删除设置文件失败: {}", e),
        }
    }

    crate::ui::build::rerender_main_ui();
    show_alert("成功", "本地授权信息已删除");
}

// ========== 公告 ==========

/// 获取公告列表
fn fetch_notice_list() {
    tracing::info!("获取公告列表...");

    // 检查是否已经在加载中
    {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.notice_loading {
            tracing::info!("公告正在加载中，忽略重复请求");
            return;
        }
    }

    // 设置加载状态
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.notice_loading = true;
    }
    crate::ui::build::rerender_main_ui();

    wit_bindgen::block_on(async move {
        // 先获取系统公告，再获取应用公告
        let url = format!("{}/api/v2/notice/Eternal", server_api_base());
        let body = serde_json::json!({});

        match super::api_client::post_json_no_auth(&url, &body) {
            Ok(json) => {
                tracing::info!("notice response: {:?}", json);
                let result = json.get("result").unwrap_or(&json);
                let notices: Vec<NoticeInfo> = if let Some(arr) = result.as_array() {
                    arr.iter().filter_map(|n| {
                        Some(NoticeInfo {
                            id: n.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            title: n.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            time: n.get("time").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            content: n.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            notice_type: n.get("type").and_then(|v| v.as_str()).unwrap_or("info").to_string(),
                            pinned: n.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false),
                        })
                    }).collect()
                } else {
                    Vec::new()
                };

                {
                    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.notice_list = notices;
                    state.notice_loading = false;
                }
            }
            Err(e) => {
                tracing::error!("获取公告失败: {}", e);
                {
                    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.notice_loading = false;
                }
            }
        }
        crate::ui::build::rerender_main_ui();
    });
}
