package plugin

import (
	ui "astroboxplugin/bindings/astrobox_psys_host_ui"
	pluginEvent "astroboxplugin/bindings/astrobox_psys_plugin_event"
	"fmt"
)

func OnEvent(eventType pluginEvent.EventType, eventPayload string) string {
	switch eventType {
	case pluginEvent.EventTypePluginMessage:
		return OnPluginMessage(eventPayload)
	case pluginEvent.EventTypeInterconnectMessage:
		return OnInterconnectMessage(eventPayload)
	case pluginEvent.EventTypeDeviceAction:
		return OnDeviceAction(eventPayload)
	case pluginEvent.EventTypeProviderAction:
		return OnProviderAction(eventPayload)
	case pluginEvent.EventTypeDeeplinkAction:
		return OnDeeplinkAction(eventPayload)
	case pluginEvent.EventTypeTransportPacket:
		return OnTransportPacket(eventPayload)
	case pluginEvent.EventTypeTimer:
		return OnTimer(eventPayload)
	default:
		appendLog(LogChannelSystem, LogDirectionNone, "event.unknown", fmt.Sprintf("type=%d payload=%s", eventType, truncateForLog(eventPayload, 240)))
		return ""
	}
}

func OnPluginMessage(eventPayload string) string {
	appendLog(LogChannelSystem, LogDirectionIn, "event.plugin-message", truncateForLog(eventPayload, 240))
	return ""
}

func OnInterconnectMessage(eventPayload string) string {
	appendLog(LogChannelInterconnect, LogDirectionIn, "event.interconnect-message", truncateForLog(eventPayload, 240))
	withState(func(state *DebugState) {
		state.InterconnectLastRecvJSON = eventPayload
	})
	RerenderMainUI()
	return ""
}

func OnDeviceAction(eventPayload string) string {
	appendLog(LogChannelDevice, LogDirectionIn, "event.device-action", truncateForLog(eventPayload, 240))
	return ""
}

func OnProviderAction(eventPayload string) string {
	appendLog(LogChannelSystem, LogDirectionIn, "event.provider-action", truncateForLog(eventPayload, 240))
	return ""
}

func OnDeeplinkAction(eventPayload string) string {
	appendLog(LogChannelSystem, LogDirectionIn, "event.deeplink-action", truncateForLog(eventPayload, 240))
	return ""
}

func OnTransportPacket(eventPayload string) string {
	appendLog(LogChannelTransport, LogDirectionIn, "event.transport-packet", truncateForLog(eventPayload, 240))
	return ""
}

func OnTimer(eventPayload string) string {
	appendLog(LogChannelSystem, LogDirectionIn, "event.timer", truncateForLog(eventPayload, 240))
	return ""
}

func OnUiEvent(eventID string, event ui.Event, eventPayload string) string {
	if !isContinuousInputEvent(eventID, event) {
		appendLog(LogChannelSystem, LogDirectionIn, "event.ui", fmt.Sprintf("id=%s type=%d payload=%s", eventID, event, truncateForLog(eventPayload, 160)))
	}

	HandleUIEvent(eventID, event, eventPayload)
	if shouldRerenderAfterUIEvent(eventID, event) {
		RerenderMainUI()
	}
	return ""
}

func OnUiRender(elementID string) {
	RenderMainUI(elementID)
}

func OnCardRender(cardID string) {
	appendLog(LogChannelSystem, LogDirectionNone, "event.card-render", fmt.Sprintf("card-id=%s", cardID))
}

func shouldRerenderAfterUIEvent(eventID string, event ui.Event) bool {
	if isContinuousInputEvent(eventID, event) {
		return false
	}
	return true
}

func isContinuousInputEvent(eventID string, event ui.Event) bool {
	if event != ui.EventInput && event != ui.EventChange {
		return false
	}
	return eventID == EventInterPayloadInput || eventID == EventAppLaunchPageInput
}
