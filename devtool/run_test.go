package main

import (
	"strings"
	"testing"
)

// TestEnvironDropsThePaneMarks holds environ to taking the pane marks off and leaving the rest of the
// environment alone. The marks are what tells `amenbo` it is inside the talk window, and a child
// devtool starts is outside it whatever pane devtool itself was typed in.
func TestEnvironDropsThePaneMarks(t *testing.T) {
	for _, mark := range paneMarks {
		t.Setenv(mark, "from-the-pane")
	}
	// The one every build reaches devtool through, and the one thing this must not take with it.
	t.Setenv("AMENBO_APP_NAME", "amenbo-dev")
	t.Setenv("DEVTOOL_TEST_KEEPER", "a=value=with=equals")

	kept := map[string]string{}
	for _, entry := range environ() {
		name, value, _ := strings.Cut(entry, "=")
		kept[name] = value
	}

	for _, mark := range paneMarks {
		if _, there := kept[mark]; there {
			t.Errorf("%s reached the child", mark)
		}
	}
	if kept["AMENBO_APP_NAME"] != "amenbo-dev" {
		t.Errorf("AMENBO_APP_NAME = %q, want amenbo-dev — only the four marks come off", kept["AMENBO_APP_NAME"])
	}
	if kept["DEVTOOL_TEST_KEEPER"] != "a=value=with=equals" {
		t.Errorf("DEVTOOL_TEST_KEEPER = %q, want the whole value — only the name is cut at the first =", kept["DEVTOOL_TEST_KEEPER"])
	}
}

// TestIsPaneMarkMatchesTheWholeName holds the match to the whole name: one that merely starts like a
// mark is not one, so the product's own build-time names go through.
func TestIsPaneMarkMatchesTheWholeName(t *testing.T) {
	for _, c := range []struct {
		entry string
		want  bool
	}{
		{"AMENBO_PANE=1", true},
		{"AMENBO_PANE_RESUME=1", true},
		{"AMENBO_SESSION=1", true},
		{"AMENBO_SESSION_DIR=/tmp/x", true},
		{"AMENBO_PANEL=1", false},
		{"AMENBO_SESSION_DIRECTORY=1", false},
		{"AMENBO_APP_NAME=amenbo-dev", false},
		{"AMENBO_PANE", false},
	} {
		if got := isPaneMark(c.entry); got != c.want {
			t.Errorf("isPaneMark(%q) = %v, want %v", c.entry, got, c.want)
		}
	}
}
