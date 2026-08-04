package plugin

import "sync"

var lifecycleOnce sync.Once

func Init() {
	lifecycleOnce.Do(func() {
		initLogger()
		OnLoad()
	})
}

func OnLoad() {
	initDebugState()
	appendLog(LogChannelSystem, LogDirectionNone, "lifecycle.on_load", "准备开始调试吧!")
}
