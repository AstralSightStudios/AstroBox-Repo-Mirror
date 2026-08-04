package plugin

import (
	device "astroboxplugin/bindings/astrobox_psys_host_device"
	dialog "astroboxplugin/bindings/astrobox_psys_host_dialog"
	interconnect "astroboxplugin/bindings/astrobox_psys_host_interconnect"
	register "astroboxplugin/bindings/astrobox_psys_host_register"
	thirdpartyapp "astroboxplugin/bindings/astrobox_psys_host_thirdpartyapp"
	transport "astroboxplugin/bindings/astrobox_psys_host_transport"
	ui "astroboxplugin/bindings/astrobox_psys_host_ui"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
)

const (
	inputConfirmButtonID = "confirm"
	inputCancelButtonID  = "cancel"
)

func DispatchUIAction(eventID string, event ui.Event, eventPayload string) {
	if event == ui.EventInput || event == ui.EventChange {
		switch eventID {
		case EventInterPayloadInput:
			actionInterPayloadInput(eventPayload)
		case EventAppLaunchPageInput:
			actionAppLaunchPageInput(eventPayload)
		case EventTransportFilterChannelInput:
			actionTransportFilterChannelInput(eventPayload)
		case EventTransportFilterTypeInput:
			actionTransportFilterTypeInput(eventPayload)
		case EventTransportSendHexInput:
			actionTransportSendHexInput(eventPayload)
		case EventTransportReqHexInput:
			actionTransportReqHexInput(eventPayload)
		case EventTransportJSONInput:
			actionTransportJSONInput(eventPayload)
		}
		return
	}

	if event != ui.EventClick {
		return
	}

	switch {
	case eventID == EventTabDeviceAndApp:
		switchDebugTab(DebugTabDeviceAndApp)
	case eventID == EventTabInterconnect:
		switchDebugTab(DebugTabInterconnect)
	case eventID == EventTabTransport:
		switchDebugTab(DebugTabTransport)
	case eventID == EventTabLogs:
		switchDebugTab(DebugTabLogs)
	case eventID == EventDeviceRefresh:
		actionDeviceRefresh()
	case strings.HasPrefix(eventID, EventDeviceSelectPrefix):
		actionDeviceSelect(strings.TrimPrefix(eventID, EventDeviceSelectPrefix))
	case eventID == EventAppRefresh:
		actionAppRefresh()
	case strings.HasPrefix(eventID, EventAppSelectPrefix):
		actionAppSelect(strings.TrimPrefix(eventID, EventAppSelectPrefix))
	case eventID == EventAppLaunch:
		actionAppLaunch()
	case eventID == EventInterEditPayload:
		actionInterEditPayload()
	case eventID == EventInterRegisterRecv:
		actionInterRegisterRecv()
	case eventID == EventInterSend:
		actionInterSend()
	case eventID == EventTransportPresetDefault:
		actionTransportPresetDefault()
	case eventID == EventTransportRegisterRecv:
		actionTransportSend()
	case eventID == EventTransportRequest:
		actionTransportRequest()
	case eventID == EventTransportToJSON:
		actionTransportToJSON()
	case eventID == EventTransportFromJSON:
		actionTransportFromJSON()
	case eventID == EventLogExport:
		actionLogExport()
	case eventID == EventLogClear:
		actionLogClear()
	default:
		appendLog(LogChannelSystem, LogDirectionNone, "ui.unknown-event", eventID)
	}
}

func switchDebugTab(tab DebugTab) {
	withState(func(state *DebugState) {
		state.CurrentTab = tab
	})
	appendLog(LogChannelSystem, LogDirectionNone, "tab.switch", string(tab))
}

func actionDeviceRefresh() {
	connected := device.GetConnectedDeviceList().Read()
	withState(func(state *DebugState) {
		state.ConnectedDevices = connected

		found := false
		for _, item := range connected {
			if item.Addr == state.SelectedDeviceAddr {
				state.SelectedDeviceName = item.Name
				found = true
				break
			}
		}
		if !found {
			state.SelectedDeviceAddr = ""
			state.SelectedDeviceName = ""
			clearSelectedAppLocked(state)
			state.ThirdpartyApps = nil
			state.InterconnectRegistered = false
			state.TransportRegistered = false
		}
	})

	appendLog(LogChannelDevice, LogDirectionNone, "device.refresh", fmt.Sprintf("connected=%d", len(connected)))
}

func actionDeviceSelect(addr string) {
	addr = strings.TrimSpace(addr)
	if addr == "" {
		appendLog(LogChannelError, LogDirectionNone, "device.select", "device address is empty")
		return
	}

	snapshot := readStateSnapshot()
	var selected device.DeviceInfo
	found := false
	for _, item := range snapshot.ConnectedDevices {
		if item.Addr == addr {
			selected = item
			found = true
			break
		}
	}

	if !found {
		appendLog(LogChannelError, LogDirectionNone, "device.select", "device not found in current list")
		return
	}

	withState(func(state *DebugState) {
		state.SelectedDeviceAddr = selected.Addr
		state.SelectedDeviceName = selected.Name
		clearSelectedAppLocked(state)
		state.ThirdpartyApps = nil
		state.InterconnectRegistered = false
		state.TransportRegistered = false
		state.TransportLastResponseHex = ""
		state.TransportLastResponseJSON = ""
	})

	appendLog(LogChannelDevice, LogDirectionNone, "device.select", fmt.Sprintf("%s (%s)", selected.Name, selected.Addr))
}

func actionAppRefresh() {
	addr, ok := selectedDeviceAddr("app.refresh")
	if !ok {
		return
	}

	ret := thirdpartyapp.GetThirdpartyAppList(addr).Read()
	if ret.IsErr() {
		appendLog(LogChannelError, LogDirectionNone, "app.refresh", "get-thirdparty-app-list failed")
		return
	}

	apps := ret.Ok()
	withState(func(state *DebugState) {
		state.ThirdpartyApps = apps

		if state.SelectedAppPackage == "" {
			return
		}
		found := false
		for _, item := range apps {
			if item.PackageName == state.SelectedAppPackage {
				state.SelectedApp = item
				state.SelectedAppName = item.AppName
				found = true
				break
			}
		}
		if !found {
			clearSelectedAppLocked(state)
		}
	})

	appendLog(LogChannelApp, LogDirectionNone, "app.refresh", fmt.Sprintf("device=%s apps=%d", addr, len(apps)))
}

func actionAppSelect(packageName string) {
	packageName = strings.TrimSpace(packageName)
	if packageName == "" {
		appendLog(LogChannelError, LogDirectionNone, "app.select", "package name is empty")
		return
	}

	snapshot := readStateSnapshot()
	var selected thirdpartyapp.AppInfo
	found := false
	for _, item := range snapshot.ThirdpartyApps {
		if item.PackageName == packageName {
			selected = item
			found = true
			break
		}
	}
	if !found {
		appendLog(LogChannelError, LogDirectionNone, "app.select", "app not found in current list")
		return
	}

	withState(func(state *DebugState) {
		state.SelectedAppPackage = selected.PackageName
		state.SelectedAppName = selected.AppName
		state.SelectedApp = selected
		state.InterconnectRegistered = false
	})

	appendLog(LogChannelApp, LogDirectionNone, "app.select", fmt.Sprintf("%s (%s)", selected.AppName, selected.PackageName))
}

func actionAppLaunch() {
	addr, appInfo, ok := selectedApp("app.launch")
	if !ok {
		return
	}

	pageName := readState(func(state DebugState) string {
		return normalizeAppLaunchPageName(state.AppLaunchPageName)
	})
	withState(func(state *DebugState) {
		state.AppLaunchPageName = pageName
	})

	ret := thirdpartyapp.LaunchQa(addr, appInfo, pageName).Read()
	if ResultUnitFailed(ret) {
		appendLog(LogChannelError, LogDirectionNone, "app.launch", "launch-qa failed")
		return
	}

	appendLog(LogChannelApp, LogDirectionOut, "app.launch", fmt.Sprintf("pkg=%s page=%s", appInfo.PackageName, pageName))
}

func actionAppLaunchPageInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}

	withState(func(state *DebugState) {
		state.AppLaunchPageName = value
	})
}

func actionInterEditPayload() {
	current := readState(func(state DebugState) string {
		return state.InterconnectPayloadJSON
	})

	value, confirmed := promptInputDialog("编辑 Interconnect Payload", "请输入 JSON 文本", current)
	if !confirmed {
		return
	}
	value = strings.TrimSpace(value)
	if value == "" {
		value = "{}"
	}
	if !json.Valid([]byte(value)) {
		appendLog(LogChannelError, LogDirectionNone, "inter.edit_payload", "invalid json payload")
		return
	}

	withState(func(state *DebugState) {
		state.InterconnectPayloadJSON = value
	})
	appendLog(LogChannelInterconnect, LogDirectionNone, "inter.edit_payload", fmt.Sprintf("payload-len=%d", len(value)))
}

func actionInterPayloadInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}

	withState(func(state *DebugState) {
		state.InterconnectPayloadJSON = value
	})
}

func actionInterRegisterRecv() {
	addr, appInfo, ok := selectedApp("inter.register_recv")
	if !ok {
		return
	}

	ret := register.RegisterInterconnectRecv(addr, appInfo.PackageName).Read()
	if ResultUnitFailed(ret) {
		appendLog(LogChannelError, LogDirectionNone, "inter.register_recv", "register-interconnect-recv failed")
		return
	}

	withState(func(state *DebugState) {
		state.InterconnectRegistered = true
	})
	appendLog(LogChannelInterconnect, LogDirectionOut, "inter.register_recv", fmt.Sprintf("pkg=%s", appInfo.PackageName))
}

func actionInterSend() {
	addr, appInfo, ok := selectedApp("inter.send")
	if !ok {
		return
	}

	payload := readState(func(state DebugState) string {
		return strings.TrimSpace(state.InterconnectPayloadJSON)
	})
	if payload == "" {
		payload = "{}"
	}
	if !json.Valid([]byte(payload)) {
		appendLog(LogChannelError, LogDirectionNone, "inter.send", "payload is not valid json")
		return
	}

	ret := interconnect.SendQaicMessage(addr, appInfo.PackageName, payload).Read()
	if ResultUnitFailed(ret) {
		appendLog(LogChannelError, LogDirectionNone, "inter.send", "send-qaic-message failed")
		return
	}

	appendLog(LogChannelInterconnect, LogDirectionOut, "inter.send", truncateForLog(payload, 240))
}

func actionTransportFilterChannelInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}
	withState(func(state *DebugState) {
		state.TransportFilterChannelStr = value
		if parsed, err := parseUint32Input(value); err == nil {
			state.TransportFilterChannelID = parsed
		}
	})
}

func actionTransportFilterTypeInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}
	withState(func(state *DebugState) {
		state.TransportFilterTypeStr = value
		if parsed, err := parseUint32Input(value); err == nil {
			state.TransportFilterTypeID = parsed
		}
	})
}

func actionTransportPresetDefault() {
	withState(func(state *DebugState) {
		state.TransportFilterChannelID = 0
		state.TransportFilterChannelStr = "0"
		state.TransportFilterTypeID = 0
		state.TransportFilterTypeStr = "0"
	})
	appendLog(LogChannelTransport, LogDirectionNone, "tp.preset_default", "channel/type reset to 0/0")
}

func actionTransportRegisterRecv() {
	addr, ok := selectedDeviceAddr("tp.register_recv")
	if !ok {
		return
	}

	filter := readState(func(state DebugState) register.TransportRecvFiler {
		return register.TransportRecvFiler{
			XiaomiVelaV5ChannelId:      state.TransportFilterChannelID,
			XiaomiVelaV5ProtobufTypeid: state.TransportFilterTypeID,
		}
	})

	ret := register.RegisterTransportRecv(addr, filter).Read()
	if ResultUnitFailed(ret) {
		appendLog(LogChannelError, LogDirectionNone, "tp.register_recv", "register-transport-recv failed")
		return
	}

	withState(func(state *DebugState) {
		state.TransportRegistered = true
	})
	appendLog(LogChannelTransport, LogDirectionOut, "tp.register_recv", fmt.Sprintf("channel=%d type=%d", filter.XiaomiVelaV5ChannelId, filter.XiaomiVelaV5ProtobufTypeid))
}

func actionTransportSendHexInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}
	withState(func(state *DebugState) {
		state.TransportSendHex = value
	})
}

func actionTransportReqHexInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}
	withState(func(state *DebugState) {
		state.TransportRequestHex = value
	})
}

func actionTransportJSONInput(eventPayload string) {
	value, ok := extractUIEventValue(eventPayload)
	if !ok {
		return
	}
	withState(func(state *DebugState) {
		state.TransportJSONInput = value
	})
}

func actionTransportSend() {
	addr, ok := selectedDeviceAddr("tp.send")
	if !ok {
		return
	}

	hexText := readState(func(state DebugState) string {
		return state.TransportSendHex
	})
	data, err := ParseHexString(hexText)
	if err != nil {
		appendLog(LogChannelError, LogDirectionNone, "tp.send", err.Error())
		return
	}

	transport.Send(addr, data).Read()
	appendLog(LogChannelTransport, LogDirectionOut, "tp.send", fmt.Sprintf("bytes=%d hex=%s", len(data), BytesToHexString(data)))
}

func actionTransportRequest() {
	addr, ok := selectedDeviceAddr("tp.request")
	if !ok {
		return
	}

	hexText := readState(func(state DebugState) string {
		return state.TransportRequestHex
	})
	data, err := ParseHexString(hexText)
	if err != nil {
		appendLog(LogChannelError, LogDirectionNone, "tp.request", err.Error())
		return
	}

	appendLog(LogChannelTransport, LogDirectionOut, "tp.request", fmt.Sprintf("bytes=%d hex=%s", len(data), BytesToHexString(data)))

	ret := transport.Request(addr, data).Read()
	if ret.IsErr() {
		appendLog(LogChannelError, LogDirectionNone, "tp.request", "transport request failed")
		return
	}

	responseData := ret.Ok()
	responseHex := BytesToHexString(responseData)
	responseJSON, jsonErr := SafeToJSONString(transport.ProtocolXiaomiVelaV5Protobuf, responseData)
	if jsonErr != nil {
		responseJSON = ""
	}

	withState(func(state *DebugState) {
		state.TransportLastResponseHex = responseHex
		state.TransportLastResponseJSON = responseJSON
	})

	appendLog(LogChannelTransport, LogDirectionIn, "tp.response", truncateForLog(responseHex, 240))
	if jsonErr != nil {
		appendLog(LogChannelError, LogDirectionNone, "tp.response.to_json", jsonErr.Error())
		return
	}
	appendLog(LogChannelTransport, LogDirectionNone, "tp.response.json", truncateForLog(responseJSON, 240))
}

func actionTransportToJSON() {
	sourceName := ""
	sourceHex := ""

	readState(func(state DebugState) struct{} {
		switch {
		case strings.TrimSpace(state.TransportLastResponseHex) != "":
			sourceName = "last-response"
			sourceHex = state.TransportLastResponseHex
		case strings.TrimSpace(state.TransportRequestHex) != "":
			sourceName = "request"
			sourceHex = state.TransportRequestHex
		case strings.TrimSpace(state.TransportSendHex) != "":
			sourceName = "send"
			sourceHex = state.TransportSendHex
		}
		return struct{}{}
	})

	if sourceHex == "" {
		appendLog(LogChannelError, LogDirectionNone, "tp.to_json", "no hex source available")
		return
	}

	data, err := ParseHexString(sourceHex)
	if err != nil {
		appendLog(LogChannelError, LogDirectionNone, "tp.to_json", err.Error())
		return
	}

	jsonText, err := SafeToJSONString(transport.ProtocolXiaomiVelaV5Protobuf, data)
	if err != nil {
		appendLog(LogChannelError, LogDirectionNone, "tp.to_json", err.Error())
		return
	}

	withState(func(state *DebugState) {
		state.TransportLastResponseJSON = jsonText
	})

	appendLog(LogChannelTransport, LogDirectionNone, "tp.to_json", fmt.Sprintf("source=%s", sourceName))
}

func actionTransportFromJSON() {
	jsonInput := readState(func(state DebugState) string {
		return state.TransportJSONInput
	})

	jsonInput = strings.TrimSpace(jsonInput)
	if jsonInput == "" {
		jsonInput = "{}"
	}
	if !json.Valid([]byte(jsonInput)) {
		appendLog(LogChannelError, LogDirectionNone, "tp.from_json", "invalid json")
		return
	}

	ret := transport.FromJson(transport.ProtocolXiaomiVelaV5Protobuf, jsonInput)
	if ret.IsErr() {
		appendLog(LogChannelError, LogDirectionNone, "tp.from_json", "transport from-json failed")
		return
	}

	hexText := BytesToHexString(ret.Ok())
	withState(func(state *DebugState) {
		state.TransportRequestHex = hexText
	})

	appendLog(LogChannelTransport, LogDirectionNone, "tp.from_json", fmt.Sprintf("bytes=%d", len(ret.Ok())))
}

func actionLogExport() {
	appendLog(LogChannelSystem, LogDirectionNone, "log.export", "generate export text")
	snapshot := readStateSnapshot()
	exportText := BuildExportLogText(snapshot.Logs)
	withState(func(state *DebugState) {
		state.ExportLogText = exportText
	})
}

func actionLogClear() {
	withState(func(state *DebugState) {
		state.Logs = nil
		state.ExportLogText = ""
		state.LastError = ""
	})
	Logf("[system] logs cleared")
}

func selectedDeviceAddr(action string) (string, bool) {
	addr := readState(func(state DebugState) string {
		return strings.TrimSpace(state.SelectedDeviceAddr)
	})
	if addr == "" {
		appendLog(LogChannelError, LogDirectionNone, action, "please select a connected device first")
		return "", false
	}
	return addr, true
}

func selectedApp(action string) (string, thirdpartyapp.AppInfo, bool) {
	snapshot := readStateSnapshot()
	addr := strings.TrimSpace(snapshot.SelectedDeviceAddr)
	if addr == "" {
		appendLog(LogChannelError, LogDirectionNone, action, "please select a connected device first")
		return "", thirdpartyapp.AppInfo{}, false
	}
	if strings.TrimSpace(snapshot.SelectedAppPackage) == "" {
		appendLog(LogChannelError, LogDirectionNone, action, "please select a target app first")
		return "", thirdpartyapp.AppInfo{}, false
	}
	return addr, snapshot.SelectedApp, true
}

func promptInputDialog(title string, content string, defaultValue string) (string, bool) {
	message := content
	if strings.TrimSpace(defaultValue) != "" {
		message = fmt.Sprintf("%s\n当前值:\n%s", content, defaultValue)
	}

	result := dialog.ShowDialog(
		dialog.DialogTypeInput,
		dialog.DialogStyleSystem,
		dialog.DialogInfo{
			Title:   title,
			Content: message,
			Buttons: []dialog.DialogButton{
				{
					Id:      inputConfirmButtonID,
					Primary: true,
					Content: "确定",
				},
				{
					Id:      inputCancelButtonID,
					Primary: false,
					Content: "取消",
				},
			},
		},
	).Read()

	if result.ClickedBtnId != inputConfirmButtonID {
		return "", false
	}

	value := strings.TrimSpace(result.InputResult)
	if value == "" {
		value = strings.TrimSpace(defaultValue)
	}
	return value, true
}

func clearSelectedAppLocked(state *DebugState) {
	state.SelectedAppPackage = ""
	state.SelectedAppName = ""
	state.SelectedApp = thirdpartyapp.AppInfo{}
	state.InterconnectRegistered = false
}

func normalizeAppLaunchPageName(value string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return defaultAppLaunchPageName
	}
	return value
}

func parseUint32Input(value string) (uint32, error) {
	parsed, err := strconv.ParseUint(strings.TrimSpace(value), 0, 32)
	if err != nil {
		return 0, fmt.Errorf("invalid uint32: %w", err)
	}
	return uint32(parsed), nil
}

func truncateForLog(value string, max int) string {
	value = strings.TrimSpace(value)
	if len(value) <= max {
		return value
	}
	return value[:max] + "..."
}

func extractUIEventValue(eventPayload string) (string, bool) {
	eventPayload = strings.TrimSpace(eventPayload)
	if eventPayload == "" {
		return "", false
	}

	type uiPayload struct {
		Value string `json:"value"`
	}

	var payload uiPayload
	if err := json.Unmarshal([]byte(eventPayload), &payload); err == nil {
		return payload.Value, true
	}

	return eventPayload, true
}
