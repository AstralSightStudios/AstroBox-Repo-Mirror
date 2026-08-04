package plugin

import (
	"fmt"
	"strings"
	"sync"

	ui "astroboxplugin/bindings/astrobox_psys_host_ui"

	"github.com/bytecodealliance/wit-bindgen/wit_types"
)

var (
	uiRootMu        sync.Mutex
	uiRootElementID string
)

func HandleUIEvent(eventID string, event ui.Event, eventPayload string) {
	DispatchUIAction(eventID, event, eventPayload)
}

func RenderMainUI(elementID string) {
	uiRootMu.Lock()
	uiRootElementID = elementID
	uiRootMu.Unlock()

	snapshot := readStateSnapshot()
	ui.Render(elementID, buildMainUI(snapshot))
}

func RerenderMainUI() {
	uiRootMu.Lock()
	elementID := uiRootElementID
	uiRootMu.Unlock()
	if elementID == "" {
		return
	}

	snapshot := readStateSnapshot()
	ui.Render(elementID, buildMainUI(snapshot))
}

func buildMainUI(snapshot DebugState) *ui.Element {
	main := makeColumn().
		Padding(16).
		Child(makeRow().JustifyCenter().Child(buildTabBar(snapshot.CurrentTab)))

	switch snapshot.CurrentTab {
	case DebugTabDeviceAndApp:
		main = main.Child(buildDeviceTab(snapshot)).Child(makeColumn().MarginTop(8)).Child(buildAppTab(snapshot))
	case DebugTabInterconnect:
		main = main.Child(buildInterconnectTab(snapshot))
	case DebugTabTransport:
		main = main.Child(buildTransportTab(snapshot))
	case DebugTabLogs:
		main = main.Child(buildLogsTab(snapshot))
	default:
		main = main.Child(makeText("未知的标签页").TextColor("#FCA5A5"))
	}

	return main
}

func buildTabBar(current DebugTab) *ui.Element {
	type tabItem struct {
		tab   DebugTab
		label string
		id    string
	}

	items := []tabItem{
		{DebugTabDeviceAndApp, "📱 设备/应用", EventTabDeviceAndApp},
		{DebugTabInterconnect, "🔗 互联", EventTabInterconnect},
		{DebugTabTransport, "⚡ 传输", EventTabTransport},
		{DebugTabLogs, "📋 日志", EventTabLogs},
	}

	bar := makeRow().
		MarginBottom(14).
		Bg("#1C1C1E").
		Radius(24).
		Padding(4)

	for _, item := range items {
		bgColor := "transparent"
		textColor := "#9CA3AF"
		if item.tab == current {
			bgColor = "#333333"
			textColor = "#FFFFFF"
		}
		bar = bar.Child(
			makeButton(item.label, item.id).
				Bg(bgColor).
				Border(0, "transparent").
				Radius(20).
				TextColor(textColor).
				Size(12).
				PaddingTop(6).
				PaddingBottom(6).
				PaddingLeft(8).
				PaddingRight(8).
				Transition("background-color 0.2s ease, color 0.2s ease"),
		)
	}
	return bar
}

func buildDeviceTab(snapshot DebugState) *ui.Element {
	panel := makePanel().
		Child(makeSectionTitle("设备信息")).
		Child(makeText(fmt.Sprintf("当前选中: %s", formatSelectedDevice(snapshot)))).
		Child(makeSecondaryButton("刷新已连接设备", EventDeviceRefresh).MarginTop(8))

	if len(snapshot.ConnectedDevices) == 0 {
		return panel.Child(makeText("暂无已连接设备").MarginTop(10))
	}

	panel = panel.Child(makeText("已连接设备列表").MarginTop(10))
	for _, item := range snapshot.ConnectedDevices {
		label := item.Addr
		if strings.TrimSpace(item.Name) != "" {
			label = fmt.Sprintf("%s (%s)", item.Name, item.Addr)
		}
		borderColor := "#374151"
		if item.Addr == snapshot.SelectedDeviceAddr {
			borderColor = "#60A5FA"
		}
		panel = panel.Child(
			makeButton(label, deviceSelectEventID(item.Addr)).
				Border(1, borderColor).
				Radius(8).
				Padding(8).
				MarginTop(6),
		)
	}

	return panel
}

func buildAppTab(snapshot DebugState) *ui.Element {
	panel := makePanel().
		Child(makeSectionTitle("第三方应用")).
		Child(makeText(fmt.Sprintf("已选设备: %s", formatSelectedDevice(snapshot)))).
		Child(makeText(fmt.Sprintf("目标应用: %s", formatSelectedApp(snapshot))))

	panel = panel.Child(makeText("启动目标页").MarginTop(8))
	panel = panel.Child(makeSingleLineInput(snapshot.AppLaunchPageName, EventAppLaunchPageInput))

	panel = panel.Child(
		makeRow().MarginTop(8).
			Child(makeSecondaryButton("刷新应用列表", EventAppRefresh).MarginRight(8)).
			Child(makeSecondaryButton("启动应用", EventAppLaunch)),
	)

	if len(snapshot.ThirdpartyApps) == 0 {
		return panel.Child(makeText("暂无应用列表，请先选择设备后刷新。").MarginTop(10))
	}

	panel = panel.Child(makeText("应用列表").MarginTop(10))
	for _, item := range snapshot.ThirdpartyApps {
		name := item.AppName
		if strings.TrimSpace(name) == "" {
			name = item.PackageName
		}
		label := fmt.Sprintf("%s (%s)", name, item.PackageName)
		borderColor := "#374151"
		if item.PackageName == snapshot.SelectedAppPackage {
			borderColor = "#60A5FA"
		}
		panel = panel.Child(
			makeButton(label, appSelectEventID(item.PackageName)).
				Border(1, borderColor).
				Radius(8).
				Padding(8).
				MarginTop(6),
		)
	}

	return panel
}

func buildInterconnectTab(snapshot DebugState) *ui.Element {
	panel := makePanel().
		Child(makeSectionTitle("应用互联")).
		Child(makeText(fmt.Sprintf("已选设备: %s", formatSelectedDevice(snapshot)))).
		Child(makeText(fmt.Sprintf("目标应用: %s", formatSelectedApp(snapshot)))).
		Child(makeText(fmt.Sprintf("注册接收状态: %s", boolText(snapshot.InterconnectRegistered))))

	panel = panel.Child(makeText("消息负载 (JSON)").MarginTop(10))
	panel = panel.Child(makeInputArea(snapshot.InterconnectPayloadJSON, EventInterPayloadInput).Height(120))

	panel = panel.Child(
		makeRow().MarginTop(10).
			Child(makeSecondaryButton("注册接收回调", EventInterRegisterRecv).MarginRight(8)).
			Child(makeSecondaryButton("发送消息", EventInterSend)),
	)

	panel = panel.Child(makeText("最后接收的负载").MarginTop(10))
	panel = panel.Child(makeCodeBlock(snapshot.InterconnectLastRecvJSON))

	return panel
}

func buildTransportTab(snapshot DebugState) *ui.Element {
	panel := makePanel().
		Child(makeSectionTitle("传输数据")).
		Child(makeText(fmt.Sprintf("已选设备: %s", formatSelectedDevice(snapshot)))).
		Child(makeText(fmt.Sprintf("注册接收状态: %s", boolText(snapshot.TransportRegistered)))).
		Child(makeText(fmt.Sprintf("信道 / 类型过滤: %d / %d", snapshot.TransportFilterChannelID, snapshot.TransportFilterTypeID)))

	panel = panel.Child(makeText("信道 ID (过滤)").MarginTop(8))
	panel = panel.Child(makeSingleLineInput(snapshot.TransportFilterChannelStr, EventTransportFilterChannelInput))
	panel = panel.Child(makeText("类型 ID (过滤)").MarginTop(8))
	panel = panel.Child(makeSingleLineInput(snapshot.TransportFilterTypeStr, EventTransportFilterTypeInput))

	panel = panel.Child(
		makeRow().MarginTop(8).
			Child(makeSecondaryButton("预设过滤 0/0", EventTransportPresetDefault).MarginRight(8)).
			Child(makeSecondaryButton("注册接收回调", EventTransportRegisterRecv)),
	)

	panel = panel.Child(makeText("待发送内容的 HEX").MarginTop(10))
	panel = panel.Child(makeInputArea(snapshot.TransportSendHex, EventTransportSendHexInput).Height(70))
	panel = panel.Child(
		makeRow().MarginTop(8).
			Child(makeSecondaryButton("仅发送 (Send)", EventTransportSend)),
	)

	panel = panel.Child(makeText("待请求内容的 HEX").MarginTop(10))
	panel = panel.Child(makeInputArea(snapshot.TransportRequestHex, EventTransportReqHexInput).Height(70))
	panel = panel.Child(
		makeRow().MarginTop(8).
			Child(makeSecondaryButton("请求 (Request)", EventTransportRequest)),
	)

	panel = panel.Child(makeText("HEX/JSON 转换区 (JSON)").MarginTop(10))
	panel = panel.Child(makeInputArea(snapshot.TransportJSONInput, EventTransportJSONInput).Height(120))
	panel = panel.Child(
		makeRow().MarginTop(8).
			Child(makeSecondaryButton("HEX -> JSON 解析", EventTransportToJSON).MarginRight(8)).
			Child(makeSecondaryButton("JSON -> HEX 编码", EventTransportFromJSON)),
	)
	panel = panel.Child(makeText("上一次响应的 HEX").MarginTop(10))
	panel = panel.Child(makeCodeBlock(snapshot.TransportLastResponseHex))
	panel = panel.Child(makeText("上一次响应的 JSON").MarginTop(10))
	panel = panel.Child(makeCodeBlock(snapshot.TransportLastResponseJSON))
	return panel
}

func buildLogsTab(snapshot DebugState) *ui.Element {
	panel := makePanel().
		Child(makeSectionTitle("运行日志")).
		Child(makeText(fmt.Sprintf("日志总数: %d / %d", len(snapshot.Logs), maxLogEntries))).
		Child(makeText(fmt.Sprintf("最后一次错误: %s", fallbackText(snapshot.LastError, "(无)"))))

	panel = panel.Child(
		makeRow().MarginTop(8).
			Child(makeSecondaryButton("清空日志", EventLogClear)),
	)

	logs := snapshot.Logs
	if len(logs) > 120 {
		logs = logs[len(logs)-120:]
		panel = panel.Child(makeText("仅显示最近 120 条日志").MarginTop(10))
	}

	panel = panel.Child(makeText("时间线").MarginTop(10))
	if len(logs) == 0 {
		panel = panel.Child(makeText("(空)").MarginTop(4))
	} else {
		for _, item := range logs {
			line := fmt.Sprintf("[%s] [%s/%s] %s | %s", item.Timestamp, item.Channel, item.Direction, item.Action, item.Detail)
			// Using selectable copyable text directly instead of standard element if supported, but here we can just use makeCodeBlock for selection
			panel = panel.Child(makeCodeBlock(line).MarginTop(4).Padding(4).Bg("transparent").Border(0, "transparent"))
		}
	}

	return panel
}

func makeTitle(value string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeP, wit_types.Some(value)).
		Size(22).
		TextColor("#E5E7EB").
		MarginBottom(10)
}

func makeSectionTitle(value string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeP, wit_types.Some(value)).
		Size(18).
		TextColor("#E5E7EB").
		MarginBottom(6)
}

func makeText(value string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeP, wit_types.Some(value)).
		Size(14).
		TextColor("#D1D5DB")
}

func makeCodeBlock(value string) *ui.Element {
	if strings.TrimSpace(value) == "" {
		value = "(empty)"
	}
	return ui.MakeElement(ui.ElementTypeP, wit_types.Some(value)).
		Size(12).
		Padding(8).
		Border(1, "#374151").
		Radius(8).
		TextColor("#CBD5E1")
}

func makeButton(label string, eventID string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeButton, wit_types.Some(label)).
		On(ui.EventClick, eventID)
}

func makeSecondaryButton(label string, eventID string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeButton, wit_types.Some(label)).
		Padding(8).
		Radius(8).
		Border(1, "#374151").
		TextColor("#D1D5DB").
		Bg("transparent").
		On(ui.EventClick, eventID)
}

func makeInputArea(value string, eventID string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeInput, wit_types.Some(value)).
		WithoutDefaultStyles().
		WidthFull().
		Padding(10).
		Radius(8).
		Border(1, "#4B5563").
		TextColor("#E5E7EB").
		On(ui.EventInput, eventID).
		On(ui.EventChange, eventID)
}

func makeSingleLineInput(value string, eventID string) *ui.Element {
	return ui.MakeElement(ui.ElementTypeInput, wit_types.Some(value)).
		WithoutDefaultStyles().
		WidthFull().
		Height(38).
		Padding(8).
		Radius(8).
		Border(1, "#4B5563").
		TextColor("#E5E7EB").
		On(ui.EventInput, eventID).
		On(ui.EventChange, eventID)
}

func makeColumn() *ui.Element {
	return ui.MakeElement(ui.ElementTypeDiv, wit_types.None[string]()).
		Flex().
		FlexDirection(ui.FlexDirectionColumn).
		WidthFull()
}

func makeRow() *ui.Element {
	return ui.MakeElement(ui.ElementTypeDiv, wit_types.None[string]()).
		Flex().
		FlexDirection(ui.FlexDirectionRow).
		AlignStart()
}

func makePanel() *ui.Element {
	return makeColumn().
		Padding(12).
		Border(1, "#374151").
		Bg("#1E1E1E").
		Radius(12)
}

func formatSelectedDevice(snapshot DebugState) string {
	if strings.TrimSpace(snapshot.SelectedDeviceAddr) == "" {
		return "(无)"
	}
	if strings.TrimSpace(snapshot.SelectedDeviceName) == "" {
		return snapshot.SelectedDeviceAddr
	}
	return fmt.Sprintf("%s (%s)", snapshot.SelectedDeviceName, snapshot.SelectedDeviceAddr)
}

func formatSelectedApp(snapshot DebugState) string {
	if strings.TrimSpace(snapshot.SelectedAppPackage) == "" {
		return "(无)"
	}
	if strings.TrimSpace(snapshot.SelectedAppName) == "" {
		return snapshot.SelectedAppPackage
	}
	return fmt.Sprintf("%s (%s)", snapshot.SelectedAppName, snapshot.SelectedAppPackage)
}

func fallbackText(value string, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func boolText(value bool) string {
	if value {
		return "已注册"
	}
	return "未注册"
}
