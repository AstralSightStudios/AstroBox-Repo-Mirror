package plugin

import (
	"log"
	"os"
	"path/filepath"
	"sync"
)

var (
	loggerOnce sync.Once
	stdoutLog  = log.New(&pluginWriter{w: os.Stdout}, "", log.LstdFlags|log.Lshortfile)
	fileLog    *log.Logger
)

func initLogger() {
	loggerOnce.Do(func() {
		if writer, ok := createFileWriter(); ok {
			fileLog = log.New(writer, "", log.LstdFlags|log.Lshortfile)
		}
	})
}

func Logf(format string, args ...any) {
	initLogger()
	stdoutLog.Printf(format, args...)
	if fileLog != nil {
		fileLog.Printf(format, args...)
	}
}

type pluginWriter struct {
	w *os.File
}

func (w *pluginWriter) Write(buf []byte) (int, error) {
	if _, err := w.w.Write([]byte("[Plugin] ")); err != nil {
		return 0, err
	}
	return w.w.Write(buf)
}

func createFileWriter() (*os.File, bool) {
	if err := os.MkdirAll("logs", 0o755); err != nil {
		return nil, false
	}

	file, err := os.OpenFile(
		filepath.Join("logs", "app.log"),
		os.O_CREATE|os.O_APPEND|os.O_WRONLY,
		0o644,
	)
	if err != nil {
		return nil, false
	}

	return file, true
}
