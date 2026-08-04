package plugin

import (
	transport "astroboxplugin/bindings/astrobox_psys_host_transport"
	"encoding/json"
	"fmt"
	"strings"
	"unicode"

	"github.com/bytecodealliance/wit-bindgen/wit_types"
)

func ParseHexString(input string) ([]byte, error) {
	var normalized strings.Builder
	normalized.Grow(len(input))

	for _, r := range input {
		if unicode.IsSpace(r) {
			continue
		}
		normalized.WriteRune(r)
	}

	raw := normalized.String()
	if raw == "" {
		return []byte{}, nil
	}

	if len(raw)%2 != 0 {
		return nil, fmt.Errorf("hex string length must be even, got %d", len(raw))
	}

	result := make([]byte, len(raw)/2)
	for i := 0; i < len(raw); i += 2 {
		hi, ok := fromHexChar(raw[i])
		if !ok {
			return nil, fmt.Errorf("invalid hex character %q at position %d", raw[i], i)
		}
		lo, ok := fromHexChar(raw[i+1])
		if !ok {
			return nil, fmt.Errorf("invalid hex character %q at position %d", raw[i+1], i+1)
		}
		result[i/2] = (hi << 4) | lo
	}

	return result, nil
}

func BytesToHexString(data []byte) string {
	if len(data) == 0 {
		return ""
	}

	var builder strings.Builder
	builder.Grow(len(data)*3 - 1)
	for i, b := range data {
		if i > 0 {
			builder.WriteByte(' ')
		}
		builder.WriteString(fmt.Sprintf("%02X", b))
	}

	return builder.String()
}

func ResultUnitFailed(ret wit_types.Result[wit_types.Unit, wit_types.Unit]) bool {
	return ret.IsErr()
}

func SafeToJSONString(protocol transport.Protocol, data []byte) (string, error) {
	jsonText := strings.TrimSpace(transport.ToJson(protocol, data))
	if jsonText == "" {
		return "", fmt.Errorf("transport to-json returned empty string")
	}
	if !json.Valid([]byte(jsonText)) {
		return "", fmt.Errorf("transport to-json returned invalid json")
	}
	return jsonText, nil
}

func BuildExportLogText(logs []LogEntry) string {
	if len(logs) == 0 {
		return ""
	}

	var builder strings.Builder
	for i, entry := range logs {
		if i > 0 {
			builder.WriteByte('\n')
		}
		builder.WriteString("[")
		builder.WriteString(entry.Timestamp)
		builder.WriteString("] [")
		builder.WriteString(string(entry.Channel))
		builder.WriteString("] [")
		builder.WriteString(string(entry.Direction))
		builder.WriteString("] ")
		builder.WriteString(entry.Action)
		if entry.Detail != "" {
			builder.WriteString(" | ")
			builder.WriteString(entry.Detail)
		}
	}

	return builder.String()
}

func fromHexChar(ch byte) (byte, bool) {
	switch {
	case ch >= '0' && ch <= '9':
		return ch - '0', true
	case ch >= 'a' && ch <= 'f':
		return ch - 'a' + 10, true
	case ch >= 'A' && ch <= 'F':
		return ch - 'A' + 10, true
	default:
		return 0, false
	}
}
