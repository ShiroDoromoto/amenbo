package main

// `devtool plugin round` — one plugin, one lap, in a store that is thrown away afterwards.
//
// A plugin author's loop is not "run the tests". It is: stand up a store nobody minds losing, put this
// build of the plugin into it by hand, fill in what its manifest declares, open its gate, make amenbo do
// the things that fire events, wait for the queues to empty, and then look at what the plugin was handed
// and how each run ended. Steps one to five are the same whichever plugin is on the bench, and they were
// being assembled by hand every time — four times over in one session, each time with a slightly different
// wait loop.
//
// So they are here, and the boundary is deliberate: **this harness makes the world and shows what came
// out.** What the plugin does with what it receives — a Slack webhook to stand in for, a git checkout to
// look at afterwards — stays with the plugin, whose author is the only one who knows what "it worked"
// means. Nothing here asserts.
//
// Two things are worth saying about the store:
//
//   - It is a throwaway base directory, named to amenbo through `AMENBO_HOME`, and removed on the way out
//     unless `--keep` is passed. Nothing points at the real app-data at any moment, so a round cannot
//     touch the plugins, the settings or the backlog on this machine.
//   - The plugin is laid down **by hand**, the way a plugin repo's own `make install` does it: a directory
//     under `plugins/` holding the manifest as `manifest.json` and the executable under the plugin's own
//     name. That is what `amenbo plugin install` produces after it has verified a signed asset, and there
//     is no signed asset for a build that has not been released — which is the whole reason this door
//     exists.
//
// With no `--program`, the plugin installed is a **stand-in of devtool's own**: a script that records the
// document it was handed and returns nothing. It is what makes "what does a subscriber actually receive"
// answerable without writing a throwaway plugin by hand, which is the other thing that kept being done.

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// How long the round waits for the queues to empty, and how often it looks. A write starts a runner of its
// own, so at the end of the firing there is usually one already working; the flush takes what nobody is on
// and the rest empties as those runners finish. A plugin that answers in the time a process takes to start
// is done in well under a second — the window is for the one that is slow on purpose, and closing it is a
// diagnosis (`the queue is still holding N`), not a crash.
const (
	roundDrainWindow = 20 * time.Second
	roundDrainPoll   = 250 * time.Millisecond
)

// roundEventKinds are the event kinds a round can fire, in the order it fires them. Named by what happened
// rather than by the event's wire name: one kind can be several events (a task deleted takes its comments
// with it), and the names on the wire are amenbo's to spell.
var roundEventKinds = []string{"created", "status", "comment", "done", "rejected", "deleted", "decision"}

func pluginCmd(args []string) {
	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	switch args[0] {
	case "round":
		pluginRound(args[1:])
	default:
		logf("devtool: unknown command %q", "plugin "+args[0])
		usage()
		os.Exit(2)
	}
}

func pluginRound(args []string) {
	fs := flag.NewFlagSet("plugin round", flag.ExitOnError)
	manifestPath := fs.String("manifest", "",
		"the plugin's manifest as JSON — the file its own hand-install lays down (required)")
	programPath := fs.String("program", "",
		"the built plugin to install (default: devtool's stand-in, which records what it is handed)")
	amenboBin := fs.String("amenbo", "",
		"the amenbo build to drive the round with (default: this checkout's debug build, built first)")
	eventSpec := fs.String("events", "all",
		"which events to fire: `all`, or a comma-separated subset of "+strings.Join(roundEventKinds, ","))
	keep := fs.Bool("keep", false, "leave the throwaway store on disk, and say where it is")
	var sets repeated
	fs.Var(&sets, "set", "a declared setting, as key=value (repeatable)")
	fs.Parse(args)

	if err := runRound(*manifestPath, *programPath, *amenboBin, *eventSpec, sets, *keep); err != nil {
		logf("devtool: %v", err)
		os.Exit(1)
	}
}

// runRound is the whole lap, so that the flag parsing above holds no logic and this can fail with an error
// rather than an exit code.
func runRound(manifestPath, programPath, amenboBin, eventSpec string, sets []string, keep bool) error {
	if manifestPath == "" {
		return fmt.Errorf("name the plugin's manifest — `devtool plugin round --manifest <path.json>`")
	}
	manifest, err := readRoundManifest(manifestPath)
	if err != nil {
		return err
	}
	kinds, err := chosenEventKinds(eventSpec)
	if err != nil {
		return err
	}
	settings, err := settingValues(manifest, sets)
	if err != nil {
		return err
	}
	if programPath == "" && runtime.GOOS == "windows" {
		return fmt.Errorf("the stand-in plugin is a POSIX shell script — pass --program on Windows")
	}
	bin, err := roundCLI(amenboBin)
	if err != nil {
		return err
	}

	base, err := os.MkdirTemp("", "amenbo-plugin-round-")
	if err != nil {
		return fmt.Errorf("make the throwaway store: %w", err)
	}
	if keep {
		logf("  store   : %s (kept)", base)
	} else {
		defer os.RemoveAll(base)
		logf("  store   : %s (removed on the way out; --keep holds it)", base)
	}
	env := []string{storeEnv + "=" + base}

	// 1. A store, a project and a binding, all three from the one command a person starts with. Every call
	//    below runs in `base`, so the binding it drops is the one they resolve against — and it goes with
	//    the directory.
	if _, err := runEnv(base, env, bin, "init", "--name", "round", "--actor", "human"); err != nil {
		return fmt.Errorf("raise the throwaway store: %w", err)
	}

	// 2. The install by hand.
	dumped := filepath.Join(base, "payload.jsonl")
	if err := layDownPlugin(base, manifest.Name, manifestPath, programPath, dumped); err != nil {
		return err
	}
	logf("  plugin  : %s installed by hand into %s", manifest.Name, filepath.Join(base, "plugins", manifest.Name))

	// 3. What its author declared, and the gate. A required setting nobody named is filled here rather than
	//    left empty, because an empty one is what `enable` refuses.
	for _, s := range settings {
		if _, err := runEnv(base, env, bin, "plugin", "config", "set", manifest.Name, s.key, s.value, "--actor", "human"); err != nil {
			return fmt.Errorf("set %s: %w", s.key, err)
		}
		if s.filled {
			logf("  setting : %s = %s (filled by devtool — --set %s=… to choose)", s.key, s.value, s.key)
		} else {
			logf("  setting : %s = %s", s.key, s.value)
		}
	}
	if _, err := runEnv(base, env, bin, "plugin", "enable", manifest.Name, "--actor", "human"); err != nil {
		return fmt.Errorf("open %s's gate: %w", manifest.Name, err)
	}

	// 4. The events, made the way an AI makes them.
	if err := fireEvents(base, bin, env, kinds); err != nil {
		return err
	}

	// 5. The queues, emptied.
	delivered, err := drainQueues(base, bin, env)
	if err != nil {
		return err
	}
	// Zero is the ordinary answer, not a failure: each write above started a runner of its own, and those
	// usually finish before the firing does. Say which happened, so a reader does not read "0" as "nothing
	// was delivered" — the payloads below are the proof either way.
	if delivered == 0 {
		logf("  drained : nothing was left — every event went out with the runner its own write started")
	} else {
		logf("  drained : %d event(s) pushed off the queues here", delivered)
	}

	// 6. What the plugin got, and how each run ended.
	if err := showDumped(dumped); err != nil {
		return err
	}
	logf("→ plugin log")
	if _, err := runThrough(base, env, bin, "plugin", "log", "--actor", "human"); err != nil {
		return err
	}
	return nil
}

// roundCLI is the amenbo the round is driven with: the one named, or this checkout's debug build, built
// first. A round is nearly always being run *for* a change in this tree, and driving it with yesterday's
// binary is how a payload ends up looking right for the wrong reason.
func roundCLI(named string) (string, error) {
	if named != "" {
		return named, nil
	}
	root := mustTreeRoot()
	if _, err := runEnv(root, cliBuildEnv, "cargo", "build", "-q", "-p", "amenbo-cli"); err != nil {
		return "", fmt.Errorf("build this checkout's CLI: %w", err)
	}
	bin := filepath.Join(root, "target", "debug", "amenbo")
	if _, err := os.Stat(bin); err != nil {
		return "", fmt.Errorf("no CLI at %s", bin)
	}
	return bin, nil
}

// layDownPlugin writes what an install would have written: the manifest under the name amenbo reads it by,
// and the executable under the plugin's own name. With no program of the caller's, devtool's stand-in is
// written instead, pointed at `dumpTo`.
func layDownPlugin(base, name, manifestPath, programPath, dumpTo string) error {
	dir := filepath.Join(base, "plugins", name)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("make the plugin's home: %w", err)
	}
	raw, err := os.ReadFile(manifestPath)
	if err != nil {
		return fmt.Errorf("read the manifest: %w", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "manifest.json"), raw, 0o644); err != nil {
		return fmt.Errorf("write the manifest: %w", err)
	}
	program := filepath.Join(dir, name+exeSuffix())
	if programPath == "" {
		return os.WriteFile(program, []byte(standInProgram(dumpTo)), 0o755)
	}
	built, err := os.ReadFile(programPath)
	if err != nil {
		return fmt.Errorf("read the plugin build: %w", err)
	}
	return os.WriteFile(program, built, 0o755)
}

// exeSuffix is what amenbo looks for the executable under — the plugin's name, plus this platform's
// suffix, which is the same rule `plugin_installed::program_file_name` follows.
func exeSuffix() string {
	if runtime.GOOS == "windows" {
		return ".exe"
	}
	return ""
}

// standInProgram is devtool's plugin: it appends the document it was handed to `dumpTo`, one line per run,
// and returns nothing.
//
// A payload arrives on stdin as one line of JSON, so appending it makes a file that is JSONL by
// construction — readable by anything, and by eye. The event's name is not written beside it: the payload
// already carries it, and a second copy would be a second thing to keep in step.
func standInProgram(dumpTo string) string {
	return "#!/bin/sh\n" +
		"# devtool's stand-in plugin (`devtool plugin round`): records what it is handed, answers nothing.\n" +
		"cat >> " + shellQuote(dumpTo) + "\n" +
		"printf '\\n' >> " + shellQuote(dumpTo) + "\n"
}

// shellQuote wraps a path for a POSIX shell. The paths here are devtool's own temp directory, but a script
// written with an unquoted path is a script that breaks on the first machine whose temp directory has a
// space in it.
func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// fireEvents does the things that make amenbo fire, as the AI facet — which is who a plugin is watching
// (an AI's writes are the ones worth reporting on).
//
// Each kind is one call, and the tasks are separate so a terminal does not have to be undone to reach the
// next: a task is done, another rejected, a third deleted. The AI may delete only what it created as the
// AI, which is what these are.
func fireEvents(base, bin string, env []string, kinds []string) error {
	fired := 0
	add := func(title string) (string, error) {
		out, err := runEnv(base, env, bin, "task", "add", "--title", title, "--actor", "ai", "--json")
		if err != nil {
			return "", err
		}
		var doc struct {
			Task struct{ ID json.Number } `json:"task"`
		}
		if err := json.Unmarshal([]byte(out), &doc); err != nil {
			return "", fmt.Errorf("read back the task's id: %w", err)
		}
		return doc.Task.ID.String(), nil
	}
	for _, kind := range kinds {
		var err error
		switch kind {
		case "created":
			_, err = add("what a plugin sees when a task is made")
		case "status":
			var id string
			if id, err = add("what a plugin sees when a status moves"); err == nil {
				_, err = runEnv(base, env, bin, "task", "status", id, "in_progress", "--actor", "ai")
			}
		case "comment":
			var id string
			if id, err = add("what a plugin sees when a comment lands"); err == nil {
				_, err = runEnv(base, env, bin, "comment", "add", id, "--text", "a comment from the round", "--actor", "ai")
			}
		case "done":
			var id string
			if id, err = add("what a plugin sees when work is carried out"); err == nil {
				_, err = runEnv(base, env, bin, "task", "done", id, "--actor", "ai")
			}
		case "rejected":
			var id string
			if id, err = add("what a plugin sees when work is decided against"); err == nil {
				_, err = runEnv(base, env, bin, "task", "reject", id, "--reason", "not this round", "--actor", "ai")
			}
		case "deleted":
			var id string
			if id, err = add("what a plugin sees when a task goes"); err == nil {
				_, err = runEnv(base, env, bin, "task", "delete", id, "--yes", "--actor", "ai")
			}
		case "decision":
			out, addErr := runEnv(base, env, bin, "decision", "add", "--title", "the round's own decision", "--actor", "ai", "--json")
			err = addErr
			if err == nil {
				var doc struct {
					Decision struct{ Ref string } `json:"decision"`
				}
				if err = json.Unmarshal([]byte(out), &doc); err == nil {
					_, err = runEnv(base, env, bin, "decision", "accept", doc.Decision.Ref, "--actor", "ai")
				}
			}
		}
		if err != nil {
			return fmt.Errorf("fire %s: %w", kind, err)
		}
		fired++
	}
	logf("  fired   : %s", strings.Join(kinds, ", "))
	return nil
}

// drainQueues pushes what is waiting through, and keeps asking until nothing is. It returns how many events
// came off the queues.
//
// One flush is not enough on its own: every write in the firing above started a runner of its own, and a
// queue one of those still holds is left alone by the flush — deliberately, since two runners on one queue
// is what the lease prevents. So this asks again while any queue still owes something, and gives up saying
// what is left rather than hanging: a plugin that never finishes is the diagnosis, not an error of the
// harness's.
func drainQueues(base, bin string, env []string) (int, error) {
	deadline := time.Now().Add(roundDrainWindow)
	total := 0
	for {
		out, err := runEnv(base, env, bin, "plugin", "flush", "--json", "--actor", "human")
		if err != nil {
			return total, fmt.Errorf("flush the queues: %w", err)
		}
		var doc struct {
			Delivered int `json:"delivered"`
			Queues    []struct {
				Plugin  string `json:"plugin"`
				Waiting int    `json:"waiting"`
			} `json:"queues"`
		}
		if err := json.Unmarshal([]byte(out), &doc); err != nil {
			return total, fmt.Errorf("read the flush back: %w", err)
		}
		total += doc.Delivered
		if len(doc.Queues) == 0 {
			return total, nil
		}
		if time.Now().After(deadline) {
			var left []string
			for _, q := range doc.Queues {
				left = append(left, fmt.Sprintf("%s holds %d", q.Plugin, q.Waiting))
			}
			return total, fmt.Errorf("the queues did not empty within %s (%s) — the plugin is slower than the window, or it never answers",
				roundDrainWindow, strings.Join(left, ", "))
		}
		time.Sleep(roundDrainPoll)
	}
}

// showDumped prints what devtool's stand-in recorded, one payload per line. Absent is the ordinary case
// when the caller installed a plugin of their own: what that one received is its own to show.
func showDumped(path string) error {
	raw, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("read what the stand-in recorded: %w", err)
	}
	var lines []string
	for _, line := range strings.Split(string(raw), "\n") {
		if strings.TrimSpace(line) != "" {
			lines = append(lines, line)
		}
	}
	logf("→ what the plugin was handed (%d payload(s))", len(lines))
	for _, line := range lines {
		fmt.Println(line)
	}
	return nil
}

// ---- the manifest, as this harness reads it ----

// roundManifest is the little of a manifest a round needs: who to install as, and what to fill in. Read as
// its own shape rather than amenbo's, because a field this does not know is a field it does not have to
// keep in step — the manifest itself is copied through byte for byte.
type roundManifest struct {
	Name   string        `json:"name"`
	Config []configField `json:"config"`
}

// configField is one setting an author declared: the key it is stored under, whether the gate refuses to
// open while it is empty, and the candidates it takes if it takes candidates.
type configField struct {
	Key      string         `json:"key"`
	Required bool           `json:"required"`
	Type     string         `json:"type"`
	Default  string         `json:"default"`
	Options  []configOption `json:"options"`
}

type configOption struct {
	Value string `json:"value"`
}

func readRoundManifest(path string) (roundManifest, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return roundManifest{}, fmt.Errorf("read the manifest: %w", err)
	}
	return parseRoundManifest(raw)
}

// parseRoundManifest reads the manifest JSON. A `.yaml` manifest is refused rather than converted: the file
// an install lays down is JSON, every plugin repo keeps one for its own hand-install, and a converter here
// would be a second reading of a contract amenbo already owns.
func parseRoundManifest(raw []byte) (roundManifest, error) {
	var m roundManifest
	if err := json.Unmarshal(raw, &m); err != nil {
		return roundManifest{}, fmt.Errorf("the manifest is not the JSON an install lays down: %w", err)
	}
	if m.Name == "" {
		return roundManifest{}, fmt.Errorf("the manifest names no plugin")
	}
	return m, nil
}

// setting is one value the round will store: the key, what goes in it, and whether the harness picked it.
type setting struct {
	key    string
	value  string
	filled bool
}

// settingValues decides what every declared setting is given.
//
// What the caller named wins. A **required** setting nobody named is filled, because the gate refuses to
// open while one is empty and a round that stops there has shown nothing. An optional one is left alone:
// empty is a state a plugin is meant to run in, and filling it would hide the branch that reads it.
//
// A `--set` naming a setting the manifest does not declare is refused rather than stored — amenbo refuses
// it too, and refusing here says so before a store is even made.
func settingValues(m roundManifest, given []string) ([]setting, error) {
	named := map[string]string{}
	for _, kv := range given {
		key, value, ok := strings.Cut(kv, "=")
		if !ok || key == "" {
			return nil, fmt.Errorf("--set takes key=value, not %q", kv)
		}
		named[key] = value
	}
	declared := map[string]bool{}
	var out []setting
	for _, f := range m.Config {
		declared[f.Key] = true
		if value, ok := named[f.Key]; ok {
			out = append(out, setting{key: f.Key, value: value})
			continue
		}
		if !f.Required {
			continue
		}
		out = append(out, setting{key: f.Key, value: fillerFor(f), filled: true})
	}
	for _, kv := range given {
		key, _, _ := strings.Cut(kv, "=")
		if !declared[key] {
			return nil, fmt.Errorf("--set %s=… names a setting %q does not declare", key, m.Name)
		}
	}
	return out, nil
}

// fillerFor is what goes into a required setting nobody named.
//
// A field with candidates takes only its candidates, so the value has to come from the field itself: the
// author's default where there is one, then every candidate for a field that takes several, and the first
// for a field that takes one. A free-text field takes a word that says where it came from — a plugin
// reading it will be handed something obviously not a real secret, which is the honest thing for a store
// that is about to be deleted.
func fillerFor(f configField) string {
	if f.Default != "" {
		return f.Default
	}
	if len(f.Options) > 0 {
		if f.Type == "multi" {
			values := make([]string, 0, len(f.Options))
			for _, o := range f.Options {
				values = append(values, o.Value)
			}
			return strings.Join(values, ",")
		}
		return f.Options[0].Value
	}
	return "devtool-round"
}

// chosenEventKinds reads the `--events` spec. `all` is every kind, in the order the round fires them; a
// subset is taken in the order the caller wrote it, since firing order is a thing worth being able to
// choose. An unknown name is refused with the list, rather than quietly firing less than was asked for.
func chosenEventKinds(spec string) ([]string, error) {
	spec = strings.TrimSpace(spec)
	if spec == "" || spec == "all" {
		return roundEventKinds, nil
	}
	var out []string
	for _, name := range strings.Split(spec, ",") {
		name = strings.TrimSpace(name)
		known := false
		for _, kind := range roundEventKinds {
			if kind == name {
				known = true
				break
			}
		}
		if !known {
			return nil, fmt.Errorf("no event kind %q — one of %s", name, strings.Join(roundEventKinds, ", "))
		}
		out = append(out, name)
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("--events named nothing")
	}
	return out, nil
}
