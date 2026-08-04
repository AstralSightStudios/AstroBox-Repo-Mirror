package plugin

import (
	device "astroboxplugin/bindings/astrobox_psys_host_device"
	thirdpartyapp "astroboxplugin/bindings/astrobox_psys_host_thirdpartyapp"
	"sync"
	"time"
)

const (
	maxLogEntries            = 300
	defaultAppLaunchPageName = "pages/index/index"
)

type DebugTab string

const (
	DebugTabDeviceAndApp DebugTab = "device_app"
	DebugTabInterconnect DebugTab = "interconnect"
	DebugTabTransport    DebugTab = "transport"
	DebugTabLogs         DebugTab = "logs"
)

type LogChannel string

const (
	LogChannelSystem       LogChannel = "system"
	LogChannelDevice       LogChannel = "device"
	LogChannelApp          LogChannel = "app"
	LogChannelInterconnect LogChannel = "interconnect"
	LogChannelTransport    LogChannel = "transport"
	LogChannelError        LogChannel = "error"
)

type LogDirection string

const (
	LogDirectionIn   LogDirection = "in"
	LogDirectionOut  LogDirection = "out"
	LogDirectionNone LogDirection = "none"
)

type LogEntry struct {
	Timestamp string
	Channel   LogChannel
	Direction LogDirection
	Action    string
	Detail    string
}

type DebugState struct {
	CurrentTab                DebugTab
	SelectedDeviceAddr        string
	SelectedDeviceName        string
	ConnectedDevices          []device.DeviceInfo
	SelectedAppPackage        string
	SelectedAppName           string
	SelectedApp               thirdpartyapp.AppInfo
	ThirdpartyApps            []thirdpartyapp.AppInfo
	AppLaunchPageName         string
	InterconnectPayloadJSON   string
	InterconnectRegistered    bool
	InterconnectLastRecvJSON  string
	TransportFilterChannelID  uint32
	TransportFilterChannelStr string
	TransportFilterTypeID     uint32
	TransportFilterTypeStr    string
	TransportRegistered       bool
	TransportSendHex          string
	TransportRequestHex       string
	TransportJSONInput        string
	TransportLastResponseHex  string
	TransportLastResponseJSON string
	ExportLogText             string
	Logs                      []LogEntry
	LastError                 string
}

var (
	debugStateMu sync.Mutex
	debugState   DebugState
)

func initDebugState() {
	withState(func(state *DebugState) {
		*state = DebugState{
			CurrentTab:                DebugTabDeviceAndApp,
			AppLaunchPageName:         defaultAppLaunchPageName,
			InterconnectPayloadJSON:   "{}",
			TransportFilterChannelID:  0,
			TransportFilterChannelStr: "0",
			TransportFilterTypeID:     0,
			TransportFilterTypeStr:    "0",
			TransportJSONInput:        "{}",
			Logs:                      make([]LogEntry, 0, 64),
		}
	})

	appendLog(LogChannelSystem, LogDirectionNone, "on-load", "debug state initialized")
}

func withState(fn func(*DebugState)) {
	debugStateMu.Lock()
	defer debugStateMu.Unlock()
	fn(&debugState)
}

func readState[T any](fn func(DebugState) T) T {
	debugStateMu.Lock()
	defer debugStateMu.Unlock()
	return fn(debugState)
}

func readStateSnapshot() DebugState {
	return readState(func(state DebugState) DebugState {
		copyState := state
		copyState.ConnectedDevices = append([]device.DeviceInfo(nil), state.ConnectedDevices...)
		copyState.ThirdpartyApps = append([]thirdpartyapp.AppInfo(nil), state.ThirdpartyApps...)
		copyState.Logs = append([]LogEntry(nil), state.Logs...)
		return copyState
	})
}

func appendLog(channel LogChannel, direction LogDirection, action string, detail string) {
	entry := LogEntry{
		Timestamp: time.Now().Format("2006-01-02 15:04:05"),
		Channel:   channel,
		Direction: direction,
		Action:    action,
		Detail:    detail,
	}

	withState(func(state *DebugState) {
		state.Logs = append(state.Logs, entry)
		if len(state.Logs) > maxLogEntries {
			overflow := len(state.Logs) - maxLogEntries
			copy(state.Logs, state.Logs[overflow:])
			state.Logs = state.Logs[:maxLogEntries]
		}
		if channel == LogChannelError {
			state.LastError = detail
		}
	})

	if channel == LogChannelError {
		Logf("[error] %s: %s", action, detail)
		return
	}
	Logf("[%s][%s] %s: %s", channel, direction, action, detail)
}
