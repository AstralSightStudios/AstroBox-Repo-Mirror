use astrobox_ng_wit::FutureReader;

use astrobox_ng_wit::exports::astrobox::psys_plugin::{
    event_v3::{self, EventType},
    lifecycle,
};

pub mod logger;
pub mod parser;
pub mod ui;

struct MyPlugin;

/// 立即返回空结果（事件回调需返回 FutureReader）
fn immediate_string() -> FutureReader<String> {
    let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(|| "".to_string());
    astrobox_ng_wit::spawn(async move {
        let _ = writer.write("".to_string()).await;
    });
    reader
}

fn immediate_unit() -> FutureReader<()> {
    let (writer, reader) = astrobox_ng_wit::wit_future::new::<()>(|| ());
    astrobox_ng_wit::spawn(async move {
        let _ = writer.write(()).await;
    });
    reader
}

impl event_v3::Guest for MyPlugin {
    fn on_event(event_type: EventType, event_payload: String) -> FutureReader<String> {
        tracing::info!("on_event: {:?}, payload: {}", event_type, event_payload);
        immediate_string()
    }

    fn on_ui_event_v3(
        event_id: String,
        event: event_v3::Event,
        event_payload: String,
    ) -> FutureReader<String> {
        tracing::info!(
            "on_ui_event_v3: event_id={}, event={:?}, payload={}",
            event_id,
            event,
            event_payload
        );
        astrobox_ng_wit::block_on(async {
            ui::handle_ui_event(event, &event_id, &event_payload).await;
        });
        immediate_string()
    }

    fn on_ui_render(element_id: String) -> FutureReader<()> {
        astrobox_ng_wit::block_on(async {
            ui::render_main_ui(&element_id).await;
            ui::refresh_device_status().await;
        });
        immediate_unit()
    }

    fn on_card_render(_card_id: String) -> FutureReader<()> {
        immediate_unit()
    }
}

impl lifecycle::Guest for MyPlugin {
    fn on_load() {
        logger::init();
        tracing::info!("Schedule Plugin 已加载");
    }
}

astrobox_ng_wit::export!(MyPlugin);
