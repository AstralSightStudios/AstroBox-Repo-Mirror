package plugin

const (
	EventTabDeviceAndApp = "tab::device_app"
	EventTabInterconnect = "tab::interconnect"
	EventTabTransport    = "tab::transport"
	EventTabLogs         = "tab::logs"

	EventDeviceRefresh      = "device::refresh"
	EventDeviceSelectPrefix = "device::select::"

	EventAppRefresh         = "app::refresh"
	EventAppSelectPrefix    = "app::select::"
	EventAppLaunch          = "app::launch"
	EventAppLaunchPageInput = "app::launch_page_input"

	EventInterEditPayload  = "inter::edit_payload"
	EventInterPayloadInput = "inter::payload_input"
	EventInterRegisterRecv = "inter::register_recv"
	EventInterSend         = "inter::send"

	EventTransportFilterChannelInput = "tp::filter_channel_input"
	EventTransportFilterTypeInput    = "tp::filter_type_input"
	EventTransportPresetDefault      = "tp::preset_default"
	EventTransportRegisterRecv       = "tp::register_recv"
	EventTransportSendHexInput       = "tp::send_hex_input"
	EventTransportReqHexInput        = "tp::req_hex_input"
	EventTransportJSONInput          = "tp::json_input"
	EventTransportSend               = "tp::send"
	EventTransportRequest            = "tp::request"
	EventTransportToJSON             = "tp::to_json"
	EventTransportFromJSON           = "tp::from_json"

	EventLogExport = "log::export"
	EventLogClear  = "log::clear"
)

func deviceSelectEventID(addr string) string {
	return EventDeviceSelectPrefix + addr
}

func appSelectEventID(packageName string) string {
	return EventAppSelectPrefix + packageName
}
