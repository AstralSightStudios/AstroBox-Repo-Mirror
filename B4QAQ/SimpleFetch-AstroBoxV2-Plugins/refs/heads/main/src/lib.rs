use wit_bindgen::FutureReader;

use crate::exports::astrobox::psys_plugin::{event_v3 as event, event_v3::EventType, lifecycle};

pub mod logger;
pub mod ui;

wit_bindgen::generate!({
    path: "wit",
    world: "psys-world-v3",
    generate_all,
});

struct MyPlugin;

impl event::Guest for MyPlugin {
    fn on_event(event_type: EventType, event_payload: _rt::String) -> FutureReader<String> {
        let (writer, reader) = wit_future::new(|| "".to_string());

        tracing::info!(
            "on_event: type={:?} payload_len={}",
            event_type,
            event_payload.len()
        );

        match event_type {
            EventType::InterconnectMessage => {
                ui::handle_interconnect_message(&event_payload);
            }
            EventType::DeviceAction => {
                // 设备连接/断开状态变化：立即刷新设备和应用列表
                tracing::info!("设备状态变化，刷新设备列表");
                ui::event_handler::refresh_device_list();
            }
            EventType::Timer => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event_payload) {
                    if let Some(payload) = json.get("payload").and_then(|v| v.as_str()) {
                        ui::handle_timer_payload(payload);
                    } else {
                        ui::handle_timer_payload(&event_payload);
                    }
                } else {
                    ui::handle_timer_payload(&event_payload);
                }
            }
            _ => {
                tracing::info!("未处理的事件类型: {:?}", event_type);
            }
        }

        wit_bindgen::spawn(async move {
            let _ = writer.write("".to_string()).await;
        });

        reader
    }

    fn on_ui_event_v3(
        event_id: _rt::String,
        ev: event::Event,
        event_payload: _rt::String,
    ) -> FutureReader<_rt::String> {
        let (writer, reader) = wit_future::new(|| "".to_string());

        ui::ui_event_processor(ev, &event_id, &event_payload);

        wit_bindgen::spawn(async move {
            let _ = writer.write("".to_string()).await;
        });

        reader
    }

    fn on_ui_render(element_id: _rt::String) -> FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        ui::render_main_ui(&element_id);

        wit_bindgen::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }

    fn on_card_render(card_id: _rt::String) -> FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        tracing::info!("on_card_render: {}", card_id);
        render_status_card(&card_id);

        wit_bindgen::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }
}

impl lifecycle::Guest for MyPlugin {
    fn on_load() -> () {
        logger::init();
        let build_time = option_env!("AB_BUILD_TIME").unwrap_or("unknown");
        let build_user = option_env!("AB_BUILD_USER").unwrap_or("unknown");
        let build_hash = option_env!("AB_BUILD_GIT_HASH").unwrap_or("unknown");
        let build_branch = option_env!("AB_BUILD_GIT_BRANCH").unwrap_or("unknown");
        tracing::info!(
            "BUILD_INFO time={} user={} branch={} hash={}",
            build_time,
            build_user,
            build_branch,
            build_hash
        );
        tracing::info!("SimpleFetch AstroBox Plugin Loaded!");

        // 注册状态文本卡片
        wit_bindgen::block_on(async move {
            let result = crate::astrobox::psys_host::register::register_card(
                crate::astrobox::psys_host::register::CardType::Text,
                crate::ui::STATUS_CARD_ID,
                crate::ui::STATUS_CARD_NAME,
            )
            .await;
            tracing::info!("register card result: {:?}", result);
        });

        // 从磁盘加载持久化配置
        let config = ui::persist::load_config();
        tracing::info!("启动：加载了 {} 个历史应用统计", config.apps.len());
        ui::state::restore_from_disk(config);

        // 预填充设备和应用缓存，并启动心跳检测
        ui::event_handler::initial_refresh();
    }
}

/// 渲染状态卡片：显示已连接设备数、已连接应用数、总请求数
fn render_status_card(card_id: &str) {
    use ui::state::{self, AppConnectionStatus};
    let devices = state::connected_devices();
    let connections = state::with_state(|s| {
        let connected = s
            .connections
            .values()
            .filter(|c| c.status == AppConnectionStatus::Connected)
            .count();
        let total_requests: u64 = s.connections.values().map(|c| c.request_count).sum();
        (connected, total_requests)
    });

    let text = format!(
        "设备: {} 台  ·  已连接: {} 个  ·  总请求: {}",
        devices.len(),
        connections.0,
        connections.1
    );
    tracing::info!("status card: {}", text);
    crate::astrobox::psys_host::ui_v3::render_to_text_card(card_id, &text);
}

export!(MyPlugin);
