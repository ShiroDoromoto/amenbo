package main

import (
	"encoding/json"
	"fmt"
	"os"
)

// amenboBin is the backlog binary. The backlog (status/show) lives in the PROD
// store reached via the repo's `.amenbo` pointer, so the default is `amenbo`.
// Override with AMENBO_BIN (e.g. amenbo-dev) for isolated testing.
func amenboBin() string {
	if b := os.Getenv("AMENBO_BIN"); b != "" {
		return b
	}
	return "amenbo"
}

// amErr mirrors amenbo's `{ "error": { code, message, hint } }` envelope.
type amErr struct {
	Code    string `json:"code"`
	Message string `json:"message"`
	Hint    string `json:"hint"`
}

func (e *amErr) Error() string {
	if e.Hint != "" {
		return e.Message + " (" + e.Hint + ")"
	}
	return e.Message
}

// task holds the fields we need to drive worktree isolation and verify the
// reservation — amenbo emits many more, and we decode only these. Double-work is
// guarded by `status` alone, so a reservation is a task sitting at in_progress.
type task struct {
	Title  string `json:"title"`
	Status string `json:"status"`
}

// setStatus runs `amenbo task status <id> <status>` in repoDir (so the `.amenbo`
// pointer resolves to the backlog store) and returns the updated task — in_progress
// reserves it, todo hands it back, and `task status --json` wraps what comes back
// as `{ ok, task: {...} }`.
func setStatus(repoDir, id, status string) (task, error) {
	out, err := run(repoDir, amenboBin(), "task", "status", id, status, "--actor", "ai", "--json")
	if err != nil {
		return task{}, err
	}
	var env struct {
		OK    bool   `json:"ok"`
		Task  task   `json:"task"`
		Error *amErr `json:"error"`
	}
	if err := json.Unmarshal([]byte(out), &env); err != nil {
		return task{}, fmt.Errorf("parse status output: %w", err)
	}
	if env.Error != nil {
		return task{}, env.Error
	}
	return env.Task, nil
}

// show runs `amenbo task show`. Unlike add/status, `task show --json` emits the
// task fields at the TOP level (no `task` wrapper), so we decode into a struct
// that embeds task alongside the optional error envelope.
func show(repoDir, id string) (task, error) {
	out, err := run(repoDir, amenboBin(), "task", "show", id, "--json")
	if err != nil {
		return task{}, err
	}
	var res struct {
		task
		Error *amErr `json:"error"`
	}
	if err := json.Unmarshal([]byte(out), &res); err != nil {
		return task{}, fmt.Errorf("parse show output: %w", err)
	}
	if res.Error != nil {
		return task{}, res.Error
	}
	return res.task, nil
}

// showHuman runs `amenbo task show <id>` in its human form and returns the text — unlike show
// (which parses --json fields), this is the operator-facing rendering that bundles the four things an
// agent must read before coding: body, notes, the linked decisions (the "why"), and the latest
// comments. task start front-loads it so the context cannot be skipped by reading notes alone.
func showHuman(repoDir, id string) (string, error) {
	return run(repoDir, amenboBin(), "task", "show", id)
}

// unreserve moves a task back to todo — how a reservation is handed back, used on
// teardown of an unfinished task.
func unreserve(repoDir, id string) error {
	_, err := run(repoDir, amenboBin(), "task", "status", id, "todo", "--actor", "ai", "--json")
	return err
}
