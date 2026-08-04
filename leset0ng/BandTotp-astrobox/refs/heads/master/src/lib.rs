use astrobox_ng_wit::exports::astrobox::psys_plugin::event_v3::{self, EventType};
use astrobox_ng_wit::exports::astrobox::psys_plugin::lifecycle;
use astrobox_ng_wit::FutureReader;

pub mod logger;
pub mod sync;
pub mod ui;

struct BandTotpPlugin;

impl event_v3::Guest for BandTotpPlugin {
    fn on_event(event_type: EventType, event_payload: String) -> FutureReader<String> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(|| String::new());

        match event_type {
            EventType::PluginMessage => {}
            EventType::InterconnectMessage => {}
            EventType::DeviceAction => {}
            EventType::ProviderAction => {}
            EventType::DeeplinkAction => {}
            EventType::TransportPacket => {}
            EventType::Timer => {}
        };

        tracing::info!("event type={:?}, payload={}", event_type, event_payload);

        astrobox_ng_wit::spawn(async move {
            let _ = writer.write(String::new()).await;
        });

        reader
    }

    fn on_ui_event_v3(
        event_id: String,
        event: event_v3::Event,
        event_payload: String,
    ) -> FutureReader<String> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<String>(|| String::new());

        tracing::info!(
            "ui event: event_id={}, event={:?}, payload_len={}",
            event_id,
            event,
            event_payload.len()
        );
        astrobox_ng_wit::block_on(async {
            ui::handle_ui_event(event, &event_id, &event_payload).await;
        });

        astrobox_ng_wit::spawn(async move {
            let _ = writer.write(String::new()).await;
        });

        reader
    }

    fn on_ui_render(element_id: String) -> FutureReader<()> {
        let (writer, reader) = astrobox_ng_wit::wit_future::new::<()>(|| ());

        ui::set_root_id(element_id.clone());
        ui::render_main_ui(&element_id);

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

impl lifecycle::Guest for BandTotpPlugin {
    fn on_load() {
        logger::init();
        tracing::info!("BandTOTP sync plugin loaded");
    }
}

astrobox_ng_wit::export!(BandTotpPlugin);
