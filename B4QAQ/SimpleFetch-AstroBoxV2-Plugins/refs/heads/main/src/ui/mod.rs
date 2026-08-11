pub mod api_client;
pub mod build;
pub mod event_handler;
pub mod icons;
pub mod persist;
pub mod state;

pub use build::render_main_ui;
pub use event_handler::handle_interconnect_message;
pub use event_handler::handle_timer_payload;
pub use event_handler::ui_event_processor;

/// SimpleFetch 文本卡片ID（显示桥接状态摘要）
pub const STATUS_CARD_ID: &str = "simplefetch-status";
/// SimpleFetch 文本卡片名称
pub const STATUS_CARD_NAME: &str = "SimpleFetch · 状态";
