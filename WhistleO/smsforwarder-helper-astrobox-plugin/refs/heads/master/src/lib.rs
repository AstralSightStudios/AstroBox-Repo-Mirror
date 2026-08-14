use wit_bindgen::FutureReader;

use crate::exports::astrobox::psys_plugin::{
    event::{self, EventType},
    lifecycle,
};

pub mod device;
pub mod logger;
pub mod sync;
pub mod ui;

wit_bindgen::generate!({
    path: "wit",
    world: "psys-world",
    generate_all,
});

struct SmsForwarderPlugin;

impl event::Guest for SmsForwarderPlugin {
    fn on_event(event_type: EventType, event_payload: String) -> FutureReader<String> {
        let (writer, reader) = wit_future::new::<String>(|| String::new());

        match event_type {
            EventType::InterconnectMessage => {
                if let Some(result) = sync::handle_interconnect_response(&event_payload) {
                    ui::apply_interconnect_result(result);
                }
                ui::refresh_ui();
            }
            EventType::PluginMessage => {
                if event_payload == "tick" {
                    ui::refresh_ui();
                } else {
                    tracing::info!("PluginMessage: {}", event_payload);
                }
            }
            EventType::DeviceAction => {
                tracing::info!("DeviceAction: {}", event_payload);
                wit_bindgen::spawn(async move {
                    sync::refresh_and_reregister().await;
                    ui::refresh_ui();
                });
            }
            EventType::Timer => {
                tracing::info!("Timer: {}", event_payload);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&event_payload) {
                    if let Some(payload) = parsed.get("payload").and_then(|v| v.as_str()) {
                        if payload == "tick" && !ui::is_dialog_open() {
                            ui::refresh_ui();
                        }
                        if payload == "loading_timeout" {
                            sync::handle_loading_timeout();
                            ui::refresh_ui();
                        }
                    }
                }
            }
            _ => {}
        };

        tracing::info!("event type={:?}, payload={}", event_type, event_payload);

        wit_bindgen::spawn(async move {
            let _ = writer.write(String::new()).await;
        });

        reader
    }

    fn on_ui_event(
        event_id: String,
        event: event::Event,
        event_payload: String,
    ) -> FutureReader<String> {
        let (writer, reader) = wit_future::new::<String>(|| String::new());

        tracing::info!(
            "ui event: event_id={}, event={:?}, payload_len={}",
            event_id,
            event,
            event_payload.len()
        );
        wit_bindgen::block_on(async {
            ui::handle_ui_event(event, &event_id, &event_payload).await;
        });

        wit_bindgen::spawn(async move {
            let _ = writer.write(String::new()).await;
        });

        reader
    }

    fn on_ui_render(element_id: String) -> FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        ui::set_root_id(element_id.clone());
        ui::render_main_ui(&element_id);

        wit_bindgen::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }

    fn on_card_render(_card_id: String) -> FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        wit_bindgen::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }
}

impl lifecycle::Guest for SmsForwarderPlugin {
    fn on_load() {
        logger::init();
        tracing::info!("SmsForwarder plugin loaded");

        wit_bindgen::spawn(async move {
            let _ = crate::astrobox::psys_host::timer::set_interval(30000, "tick").await;
        });

        wit_bindgen::spawn(async move {
            match sync::bootstrap_sync().await {
                Ok((_registered, msg)) => {
                    tracing::info!("bootstrap_sync: {}", msg);
                    ui::update_status_direct(&msg);
                    ui::refresh_ui();
                }
                Err(e) => {
                    tracing::warn!("bootstrap_sync failed: {}", e);
                    ui::update_status_direct(&format!("启动注册失败: {}", e));
                    ui::refresh_ui();
                }
            }
        });
    }
}

export!(SmsForwarderPlugin);
