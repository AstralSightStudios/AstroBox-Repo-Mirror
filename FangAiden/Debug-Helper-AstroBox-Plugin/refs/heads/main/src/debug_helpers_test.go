package plugin

import (
	"reflect"
	"testing"

	"github.com/bytecodealliance/wit-bindgen/wit_types"
)

func TestParseHexStringSuccess(t *testing.T) {
	got, err := ParseHexString("0A FF 01")
	if err != nil {
		t.Fatalf("ParseHexString returned error: %v", err)
	}

	want := []byte{0x0A, 0xFF, 0x01}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("unexpected bytes, got=%v want=%v", got, want)
	}
}

func TestParseHexStringFailOddLength(t *testing.T) {
	_, err := ParseHexString("0A F")
	if err == nil {
		t.Fatalf("expected odd length error")
	}
}

func TestParseHexStringFailInvalidChar(t *testing.T) {
	_, err := ParseHexString("0A FG")
	if err == nil {
		t.Fatalf("expected invalid character error")
	}
}

func TestBytesToHexString(t *testing.T) {
	got := BytesToHexString([]byte{0x0A, 0xFF, 0x01})
	want := "0A FF 01"
	if got != want {
		t.Fatalf("unexpected hex string, got=%q want=%q", got, want)
	}
}

func TestResultUnitFailed(t *testing.T) {
	ok := wit_types.Ok[wit_types.Unit, wit_types.Unit](wit_types.Unit{})
	err := wit_types.Err[wit_types.Unit, wit_types.Unit](wit_types.Unit{})

	if ResultUnitFailed(ok) {
		t.Fatalf("ok result should not be failed")
	}
	if !ResultUnitFailed(err) {
		t.Fatalf("err result should be failed")
	}
}
