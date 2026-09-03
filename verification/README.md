# Pre-distribution verification

This subsystem verifies the **shipped / installed** Amenbo build as a black box, before a
release goes out. It is deliberately separate from `make test`, which exercises the
build-time workspace artifacts.

The single source of truth is the set of **scenarios**: declarative YAML naming what is being
proved and the road each driver takes to prove it, with no command line or coordinates baked in.
The goal is shared; the steps belong to the driver that walks them (`steps_cli` / `steps_gui`).

**What the drivers drive is the shipped bytes, and nothing else.** Each asks before it starts —
the CLI driver asks the binary `--bin` names, the GUI harness asks the CLI the bundle it was
pointed at ships — and the answer is the release workflow's stamp, which a build reports as
`release_build` in what `amenbo version --json` says. There is no flag to wave a local build
through: evidence gathered from a shipped build and evidence gathered from somebody's working tree
read alike afterwards, and a promotion resting on the second rests on nothing. Reading a road back
is not driving one, so `lint`, `emit` and `verify-gui --print` sit outside that line — they touch no
binary at all. To see a change you are writing on screen, build and drive its own dev GUI with the
development tooling; that is what it is for, and it is not this harness's road.

```
verification/
  scenarios/   the single source of truth (YAML). Every driver walks its own road through these.
  fixtures/    what a scenario cannot hold itself (a file carrying an amenbo ref for the lint, an image a picker is pointed at)
  core/        the scenario schema + validating loader (crate `amenbo-scenario`, `lint` + `emit` bins)
  cli/         CLI driver + runner — drive the shipped binary, assert via --json (crate `amenbo-verify-cli`)
  gui/         mac harness — scenario → screen checklist, shot and read by the screen tool (crate `amenbo-verify-gui`).
               It reaches into `cli/` for the two things a screen run needs of the binary: the
               throwaway store, and the vocabulary a `given:` world is stood up in.
```

`core/`, `cli/` and `gui/` are members of this cargo workspace, outside the main workspace, so the
root manifest's own clippy and tests never reach them. What does is a stage of its own on either
side: CI's `verification` job, gated to changes under `verification/` (`.github/workflows/_ci.yml`),
and `make gate-verification` locally, which `make gate` runs when this layer is what a change
touched and `make test` runs unconditionally. Both are the same two lines:

```sh
cargo clippy --manifest-path verification/Cargo.toml --all-targets -- -D warnings -A clippy::disallowed_methods
cargo test --manifest-path verification/Cargo.toml
```

`-A clippy::disallowed_methods` is not slack, it is the one rule that cannot apply here: this
workspace black-box-drives the shipped binary, so it reads process env raw (`AMENBO_BIN`) rather
than through `amenbo_core::env`, which it has no dependency on. A plain `cargo clippy
--all-targets` in this directory fails on those lines and on nothing else.

## One scenario per path a reader walks

A scenario is **one goal and the steps that reach it** — the path a reader actually walks, not a
feature out of a list. What this gate defends is the release, and what a release breaks is a path.

- **The file is named after the path**, and the scenario's `id` is the file's stem. Nothing pins the
  set's size or its names to Amenbo's own capability list: that list stays the feature inventory
  `amenbo agent --json` prints, and it is not the denominator of anything here.
- **A step belongs to the path it is on**, whichever commands it takes on the way. A path crosses
  several capabilities by definition — that is what makes it a path — so the question a step answers
  is "does this get the reader to the goal", never "which command owns this line".
- **A goal both the CLI and the screen can reach is one file**, carrying both roads. A goal only one
  of them can reach — what only the screen ever says, what has no screen at all — gets that driver's
  file alone, and is not a gap in the other.

The set is kept honest by the hand that changes the product, not by a count: add, change or drop a
path a reader walks, and its scenario is written, fixed or dropped in the same session.

One rule sits above all of that: **a line that needs an op the registry does not have is not written
here at all.** Growing the registry means growing every driver's mapping with it, which is its own
implementation in its own workspace; a YAML that runs ahead of it turns `verify-all` red and holds
up a release. File a task for the op instead, and leave the line out until it lands.

## CLI driver

`verify-cli` reads one scenario, maps each step to an invocation of the **shipped / installed**
`amenbo` binary, and judges the asserts from that binary's `--json` output — a black box, so it
knows the domain vocabulary, not the build under test.

```sh
cd verification
# drive a specific binary (e.g. the CLI extracted from a release .pkg):
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/delegate-to-ai.yaml --bin /path/to/amenbo
# or the `amenbo` on PATH (the installed CLI), with a machine-readable result:
cargo run -p amenbo-verify-cli --bin verify-cli -- scenarios/delegate-to-ai.yaml --json
```

`--bin` (and `$AMENBO_BIN`) takes a relative path as well, read from where you run the command and
not from the throwaway directory the scenario is driven in — so a path to an artifact you unpacked
beside the repository means what you see. A value with no separator in it (`--bin amenbo`) stays a
`PATH` lookup. A binary the release workflow did not produce is refused by name, before the first
step of the first scenario.

The run is isolated by `AMENBO_HOME` pointed at a throwaway store plus a `.amenbo`-free CWD;
the real app-data is never touched, and `AMENBO_UPDATE_CHECK=0` keeps it off the
network. Exit code is the machine signal — `0`
when every assert passes, non-zero on any failed assert or execution error — so the runner reads it
directly. `--keep` leaves the throwaway store in place for inspection.

**One thing does leave the box.** `plugin install` resolves the official catalog over the network,
picks this platform's asset and verifies its signature against the key built into the binary — a
layer that exists only in a shipped build, and one no local fixture can stand in for without the
run reading green over the very thing it exists to catch. The plugin scenarios therefore need
the network and an intact catalog — the ones that install one, and the one that reads the browsing
view back — and they are the only ones that do.

The loopback is the other side of that. A third-party catalog is trusted on the signing key it
publishes beside its `catalog.json`, and a key is *served*, never written down — so a scenario that
walks the pin (`plugin catalog-stand`) has the run publish a catalog of its own on a port, and names
it by the `as:` binding rather than by a URL it could not have known. `catalog-rotate-key` serves the
other of two keys from the same address, which is a publisher rotating theirs as Amenbo sees it. The
host is `amenbo-static-host`, shared with the main workspace's own tests.

What that catalog puts on its shelf is the scenario's to write, under `offers:` — the one arg written
as a list of rows rather than as a word. Each row is the words a catalog's own documents carry: the
`name` it is fetched and badged by, the `desc` drawn under it, the `claims_official` badge it is not
entitled to (a shelf anyone may publish into holds no review, so seeing the merge clear that claim
needs one to have been made), and the one `setting` its author declares under the `label` a form
shows. Naming no rows is an empty shelf, which is what a road about the trust root alone wants. A
registration takes a `name:` for the same reason the rows are named at all: a catalog registered
without one is called after the host of its URL, and a loopback address with a port picked this run
is nothing a road can read a row's provenance back by. Nothing on such a shelf installs — the rows
carry no asset, and an install is walked against the real catalog, whose signature is the layer no
fixture can stand in for.

A row may also carry `about:` — what its author wrote about the plugin at length, which is the body an
opened panel is read by — and `translated:`, the same `desc`, `about` and `label` as its author wrote
them in other languages, keyed by language code. What the shelf then publishes is the shape a real
catalog publishes, which is two shapes rather than one. The lines go beside the list, one
`catalog.<lang>.json` per language, so a reader fetches their own and nobody pays for the other
eighteen; the description text and the labels go inside each row's own detail document, every language
at once, so a panel and a form already fetched follow a language change with no request behind it. That
split is the reason to write a row this way rather than to stand a shelf per language: what a screen
road has to be able to see is a listing redrawn from a document fetched for the new language, and a
panel redrawn from one nobody fetched again. A language no row drew a *line* in gets no document, and
the 404 the fetch meets is the answer a reader of an untranslated language already gets — so the
fallback is walked by leaving a row, or a whole shelf, untranslated rather than by breaking anything.
A row that carries no `about:` at all is the other fallback, one layer up: its panel is drawn from the
README of the repository every stood row names, which is where a plugin's description came from before
authors had anywhere else to write one.

Each op the driver maps is an arm in its domain's module under `cli/src/domain/` — `task depend` in
`task.rs`, `plugin install` in `plugin.rs` — and an op that is in the scenario registry but not yet
mapped fails loudly rather than passing silently. `cli/src/lib.rs` keeps only what every arm stands
on (the isolated session, the one invocation, the bindings, the report) and hands each step to the
domain it names, so "how is this op driven?" is answered by opening one file.

## Runner

`verify-all` drives a whole set of scenarios through that same driver, one after another, and
rolls their verdicts into one: green only when every scenario is green. Each scenario runs in its
own throwaway store, and a scenario that fails to load or whose binary errors is recorded as a red
entry — the run carries on, so one broken scenario never hides the rest.

```sh
cd verification
# every scenario under scenarios/ (the default), against a specific binary:
cargo run -p amenbo-verify-cli --bin verify-all -- --bin /path/to/amenbo
# a chosen subset (files and/or directories), with a machine-readable aggregate:
cargo run -p amenbo-verify-cli --bin verify-all -- scenarios/shape-a-task.yaml scenarios/delegate-to-ai.yaml --json
```

A scenario with no `steps_cli` road is **skipped**: printed as skipped, counted apart, and left
out of the verdict. If that leaves nothing to run, the exit is non-zero rather than
an empty green — a gate that verified nothing must not read as one that verified everything.

The exit code is the roll-up — `0` when every scenario that ran is green, non-zero when any is red
or errored — so a release gate reads it directly. The `--json` aggregate carries `total` / `passed`
/ `failed` / `skipped` / `green` plus each scenario's own report (or its error, or the drivers
whose roads it carries).

## GUI harness (mac)

`verify-gui` walks a scenario's `steps_gui` road as a **screen checklist**, and only a scenario
that has one — it refuses the rest by name instead of shooting a road written for the binary. It
bakes in no command line and no pixel: each step becomes a plain-language instruction of what to do
or confirm on screen, and the shooting is the screen tool's (`scripts/screen.swift`) — the harness
names the app by pid and receives one file per step in an evidence directory (plus a
`manifest.json` pairing each instruction, verdict and shot). Which window was shot, and the id it
was shot by, stay inside the tool: a format nobody is handed is a format nobody parses.

**The run owns the app it shoots.** It launches the `.app` bundle named by `--app`, with
`AMENBO_HOME` pointed at a throwaway store of its own, holds the pid that launch answered with, and
takes both down when it ends. Two things follow, and both are the point of doing it this way.
Nothing separates a shipped build started for a run from the same shipped build the user keeps
open — one executable name, one bundle, no badge on screen, and nothing stopping both from running
at once — so a run that went looking for a process could shoot either, and the evidence it filed
would read the same. And a screen road creates projects, tasks and bindings, none of which belong
in the store the operator actually works in; a store the run makes and drops leaves nothing for
anyone to remember to tidy.

The executable inside the bundle is started directly rather than the bundle being `open`ed, since
the environment is what carries the store and `open` hands the launch to launchd with an
environment of its own. `AMENBO_HOME` is the product's own override, so the build under test is not
a different build for having been asked. `AMENBO_UPDATE_CHECK=0` rides along with it, the same way
it does on the CLI side: the app asks the release manifest as it comes up, and a road walked over
and over would otherwise put every one of those launches into the numbers the product is measured
by. The store follows this workspace's throwaway rules — one parent under the temp tree, a name
that does not lean on the pid, a sweep of what is over a day old on the way in — and the app is
started from a directory of its own, since a child inherits the harness's, and the harness is run
from this repository.

**The world a road starts from is stood up before the app is.** A scenario's [`given:`](#given--the-world-a-road-starts-from)
is walked by the CLI the same bundle ships (`Contents/MacOS/amenbo`), pointed at that same throwaway
store: the build under test stands up its own world, rather than whichever Amenbo the operator has
on `PATH`. It runs before the launch, since a store is read as the app comes up, and before there is
an evidence directory, so a premise that could not be stood up leaves no shots of a half-built
world. Where it stops is the screen's own moves — `steps_gui` is the operator's to walk, and a
driver that carried those out would leave the road proving something already done. A scenario that
declares no world gets none and opens on an empty store. What was stood up is written into
`manifest.json` under `world`, because the store it describes goes out with the run — and it is said
on stderr before the first step is handed over, because part of it is the operator's to reach for. A
file a premise wrote lies under a throwaway path, and the instructions are rendered from the YAML
alone, so nothing in the road can name it: the line that says where it landed is the only way to
find it in a picker.

An assert is judged by asking that same tool to read the shot back (macOS's own **Vision** behind
it): the harness derives the text the step expects on screen — for a `listed` assert, the bound
task's title — and passes when it is present in the reading (or absent, for `present: false`).
Both sides meet on their words rather than their glyphs — case, punctuation and the line a wrapped
card broke on are folded away, and so is the long vowel mark Vision returns where a title carries a
dash, which Unicode files under letters. That fold is the reader's habit, so the tool applies it to
what it read and hands back the unfolded reading as well; the harness folds its own expectation by
the same rule and matches. The reading as it came back is written next to the shot (`NN-…​.txt`),
which is what a person reads when a step comes out red.

**One character inside those words is forgiven, and nothing more.** Vision reads the words on a
screen well and the glyphs inside them not always — `day's` came back as `dav's` on a title that was
otherwise perfect, and the fold keeps alphanumerics, so a verbatim search finds nothing. The
expectation is therefore matched with a budget of one edit over the whole of it, and only where the
folded expectation is at least 8 characters: under that a single edit is most of the word, and two
values a scenario tells apart (`core`, `gore`) are exactly that far from each other. The budget is
counted in characters rather than in words because the screen is also read in Japanese, where the
fold leaves a title with no spaces to count. Two misreads in one title is not what this is for —
that shot goes red and a person reads it. Which way it leans is the reassuring part: the same
tolerance that finds a misread title on a `present: true` step finds it on a `present: false` step
too, so it can red a run and never green one on a screen nobody stood up. A step that passed only
because a character was forgiven says so in the summary and carries `slipped` in `manifest.json`;
several of those in one run is a reader going wrong rather than a screen.

**Two pairs of glyphs are folded onto each other before any of that**: the digit `1` against the
letter `l`, and the digit `0` against the letter `o`. They are one drawing, not a reader's slip, so
they cost nothing out of the budget above and reach the expectations the budget cannot — a category's
key is a monospace word of five or six characters, well under the floor, and `channel` came back as
`channe1` off a shot it was plainly legible on. What it gives up is telling `route1` from `routel`,
which no reading of a photograph could do anyway. A lowercase `i` is deliberately not in the set: the
face this serves draws it with a dot, so folding it onto `l` would give away discrimination against a
misreading this screen does not produce. A green earned this way carries `slipped` like any other.
An assert OCR cannot mechanically judge — a structured `field` value — is a `Review`: its shot is
kept for an AI/human eye and does not fail the run. A task's **title is one of those once the task
has ended**: done and rejected are drawn with a line through them, and the reader returns the glyphs
under that line as other letters (`SCENARIO — work is over` came back as `SCENARIOwotk is eveF`), so
no fold brings the two sides together. The harness follows each binding through its terminal states
and leaves such a step for an eye, saying so in the instruction. The half worth knowing is the
absent one: a reading that cannot find a title it is looking straight at passes a `present: false`
step, so those lines read green while proving nothing — which is what this takes away. Write the
machine-judged half of a road on cards that are still open. tesseract stays the Linux container path
(`scripts/docker/gui-e2e.sh`); each driver walks the road written for it.

The Linux container carries no toolchain, so it can't read the scenario itself. Its host launcher
(`make verify-gui-linux`) resolves the scenario through the `emit` bin and passes the card — the
`listed`/present title — into the container as `AMENBO_E2E_CARD`. tesseract reads the words but not
every glyph, so that path matches the title on its alphanumerics, not verbatim. `SCENARIO` selects
which scenario drives it (default `scenarios/delegate-to-ai.yaml`).

```sh
cd verification
# the crate's JSON face over the validated model — a shell consumer reads it through jq,
# never by reparsing YAML:
cargo run -p amenbo-scenario --bin emit -- scenarios/delegate-to-ai.yaml
```

```sh
cd verification
# launch the installed bundle against a throwaway store and shoot one shot per step:
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/delegate-to-ai.yaml \
  --app ~/Applications/Amenbo.app
```

`--app` is the bundle itself, not a name: what is verified before a release is the `.app` the
installer put in place, and the bundle is asked whether it is one — through the CLI it ships, which
one build produces alongside the app and one installer carries with it. The executable to start is
asked of the bundle too (`CFBundleExecutable`) rather than assumed.

The screen tool is the input primitive too, called by whoever drives the screen between steps: its
`find` / `click-named` / `click` / `dblclick` / `type` / `key` / `scroll` / `set-date` carry out the action
steps the checklist names. The run holds itself at the launch until the app is up, in front, and can be shot
at all — the proof it waits for is a shot it throws away, since an app the system has taken up is
not yet an app with a window, and a walk that started between the two would fail on its first step.
An app that never draws one inside a minute is reported as that, and one that exits on the way up is
reported the moment it does. `--evidence <dir>` chooses where
the shots and manifest land (default: a fresh dir under the temp tree); `--screen <path>` points at
a tool other than the repo's own. Exit is 0 when every OCR-judged assert passed and every step was
captured, non-zero on a failed assert or a load/capture/reading failure — a `Review` step is closed
by a human from the evidence, not by the exit code.

**Name what to press rather than aim at it.** `swift scripts/screen.swift find <pid>` lists every
element on screen with the name it answers to and where it stands, and `click-named <pid> <name>`
clicks the one of that name — bringing that pid's app to the front first, since a press lands on
whatever is frontmost where it is aimed and anything that took the front would swallow it silently.
The screen is a webview, so both read it through the accessibility tree the app serves once asked.
A part of a name will do — the name an element answers to is not the label
on the screen (an emoji in front of the words belongs to it, and a card folds its lines into one
string), so a whole one is rarely knowable in advance. When several names hold what was asked for,
the tool prints them and presses nothing.
A point worked out from a shot's pixels carries two errors instead: the shot's pixels are the window's
points times the scale of the display it was on (2 on a built-in panel, 1 on an external one), and the
screen goes on moving after the shot — opening the right pane pushes a column header down by tens of
pixels. Anything wide swallows both, which is why aiming works until it is aimed at something small:
the board's `＋` and the view tabs read as unreachable elements until the arithmetic was suspected
instead.

**A page is walked with `scroll <pid> <dx> <dy>`, not with the keys.** Page Down is the one scrolling
key that reaches the webview — Page Up, Home, End and the arrows were posted the same way and nothing
moved — so a road that went down a pane had no way back up to what it had passed, and reopening the
pane does not reset it either, the position being kept. A wheel arrives where those keys do not.
Positive is the way back: `scroll <pid> 0 800` goes 800 points up the page, and toward its left
across. The pointer is put in the middle of the window first, since a wheel lands where it is
pointing rather than on whatever holds focus; something else on the screen that scrolls is reached by
clicking into it and scrolling after.

**A day goes in through `set-date <pid> <name> <yyyy-mm-dd>` rather than through the keys.** A date
field is one control with three numbered parts in it, and typing into it is a digit at a time — but
every digit that leaves the value a valid day makes the app commit and redraw the field, and the
redraw drops the run of digits the webview was collecting. A year is four digits and valid after each
one, so `2099` arrives as `0009`. Typing slower does not help: it is the redraw between the digits,
not the pace of them. The tool opens the field's picker, writes the day where the picker keeps it,
tabs back out so the panel is not standing over the rows beneath, and reads the field back — so a
write that reached nothing is reported here rather than as a red assert on the day.

**`--near <name>` says which row, where the field's own name does not.** A manager listing the values
of an axis draws every row the same `Start date` and `End date`, so the name reaches one field per
value and asking it to name itself more fully asks for a name that is not there. What is there is the
value's own name, on that row: `set-date <pid> "Start date" 2001-01-01 --near Ongoing` writes into the
`Start date` standing beside `Ongoing`. Same row is read off the frames the tree answers with at that
moment — no shot, no scale, nothing that a screen having moved could put out — and a `--near` that
leaves several fields, or none, is refused the same way an ambiguous name is.

### `--print` — read the road without a screen

The screen road is written in YAML, but the sentences it turns into are written in Rust, so what a
step will actually say to an operator cannot be read off the file it was written in. `--print`
answers that: it renders the road and prints the instructions, one to a line, and stops there — no
app is launched, nothing is shot, no OCR runs, and no GUI has to be built first (`--app` is not
even asked for).
The lines are the very text a run hands the operator, not a second rendering that could drift from
it, and a road carrying an op the harness has not mapped fails here exactly as it would mid-run.
A scenario with no `steps_gui` is refused by name, the same as it is for a real run.

```sh
cd verification
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/link-a-folder.yaml --print
```

### Every step is handed over before it is shot

A screen road is walked by somebody, always. The run prints the step it is about to shoot and waits
for a line on stdin; between the two, the screen belongs to whoever is driving — carry the step out
by hand, or with the screen tool's `click-named` / `type` / `key` / `scroll` / `set-date`, and send the line
once the screen is standing where the step says it should. There is no flag for running it any other way.

**The hand-over comes before the step, the first one included.** That is what lets a road open with a
check: a run starts on a store made for it a moment ago, so the screen a launch leaves behind is
whatever a store nobody has used yet opens on — the hooks question over an empty board — and a road
whose opening line reads the board would be judged against that. Handed the step first, the driver
stands the screen where the line says before anything is captured.

Left to itself a run would photograph one screen as many times as the scenario is long, and pass:
the verdict is a substring in what OCR read off the shot, so the screen before a step and the screen
after it are not told apart, and a line asking that something *not* be on screen passes for as long
as nothing moves. A step nobody carried out is the one thing this harness cannot see, which is why it
never runs without somebody there.

The wait is on a line and not on a clock on purpose. A run held for a fixed number of seconds shoots
whatever is up when the clock runs out, so a step that took a moment longer is filed as evidence of a
screen nobody stood on — a red nobody can tell from a real one, or worse, a green. For the same
reason end of input is a failure and not a nod: a run with nothing left to hold it would walk the
rest of the scenario off one screen and report it as though it had been driven.

What an assert expects OCR to find is shown with it at the hand-over. The reading is a substring
match and nothing more, so the driver — who can see the screen — is the one who can tell a check that
genuinely passed from one the words happened to satisfy, and say so when the evidence is read back.

The moves themselves are written in the scenario, not in a note beside it. Getting from one screen
to the next is an action step like any other (`folder open-existing-card`, `folder choose-project`),
and it earns the two things a note never can: the screen it arrives at is shot, so the middle of the
road is evidence rather than something taken on trust, and the assert after it cannot be reached by a
hand tidying the screen while the run is held. The screen roads are written that way: `link-a-folder`
walks the arrival screen, the card, the picker and the board it lands on,
`say-where-a-plugin-fires` walks one plugin's row through the four states one switch leaves it in,
`learn-a-plugin-reads-more-than-this-project` walks two rows on that same screen for the one thing a
declaration says and a switch cannot — a plugin declared the machine's says in words that it reads the
whole device, and the plugin beside it that declared nothing says nothing —
`put-a-device-wide-plugin-to-work` walks that first plugin's one row all the way through — the mark it
wears, the settings opened inside it, the press, and then a project's own settings offering no second
switch for it —
`put-a-plugin-to-work-from-the-project` walks the same crossing from the other face — the arrival, the
picker that draws the row, and the switch inside it —
`fill-in-what-a-plugin-cannot-fire-without` walks one row from the mark it wears through the refusal, the
settings opened inside it and the press that then goes through,
`learn-why-a-plugin-will-not-turn-on` walks the gate that turns on the author's own judgement — the two
sentences a refusing check puts on the form after a save, the value staying saved and the plugin staying on
under them, and the same check standing in front of the switch when it is pressed again —
`see-only-the-settings-that-apply` walks the same form one layer up — the candidate an author withheld,
the field that candidate gates and the operation acting on it, each absent until the answer above it is
given, and the field and its button arriving together —
`choose-from-what-a-plugin-offers` walks a settings form through the three answers a choice holds and
the button back to the author's default, `press-what-a-plugin-offers-to-do` walks the other half of that
same form — the operation its author declared, drawn but unpressable while the gate is shut, the box it
asks for coming up at the press, the author's own line coming back with what was typed in it, the
second press finding that box empty again, and the button beside it asking for a credential through a box
that draws none of it back — `finish-writing-a-task-before-anyone-takes-it` walks the one
premise a reader settles where it is reported — the card drawn while its creation is still open, and
the button inside it that ends the creation, `read-a-plugin-in-your-own-language` walks a listing
across a language change — the line one author wrote in the reader's language, the row beside it whose
author wrote none, the shelf that published no document for that language at all, and the panel whose
label came down with every language inside it — `read-a-plugins-form-in-your-own-language` walks the
same change on the other side of an install, where the words were never published anywhere and so
cannot have been fetched: the field translated, the field beside it that is not, and the candidate drawn
under one set of words while storing the value it always stored — `learn-what-a-plugin-does-before-installing-it`
walks the third of those doors, the panel an opened row is read by: the author's own description standing
there with no README beside it, the same description again after a language change, the row whose author
wrote one language keeping it unmarked, and the row nobody described at all falling back to the
repository's README — `see-a-tasks-classification-on-the-board` walks what a card says of how its task
is filed, on a board whose world was classified from a terminal: the value drawn for the axis its
project put on the card, and nothing at all from the axis beside it that did not ask —
`answer-one-category-with-several-values` walks the same board over a category one task answers twice:
both values on the one card, the category missing from the row of buttons that cut the columns while
the category beside it is still offered there, and the pair still drawn once the board is cut along
that other one —
`file-work-by-moving-its-card` walks that board as the place work is filed from rather than read on —
the column standing before anything is in it, a card carried into it and another carried elsewhere,
and the narrowing afterwards that comes back with the first and leaves out the second —
`set-an-app-up-to-reach-this-project` walks the fold that holds the other way in — the app already reaching this
project and the folder its entry names, beside the app reaching nothing, each row offering the one road its app can
walk —
`be-offered-a-start-at-login`
walks the one thing on screen nobody went and asked for — an offer that comes up on its own once the app has been come
back to, and the answer that takes it away — and the hourly check's roads close the set:
`be-offered-the-hourly-check` walks the band that arrives when a dated task, a carrier and an unanswered
device stand together, the yes read back on the settings row, and that row taking it back off;
`take-back-a-no-about-the-hourly-check` walks the way back a declined band, rightly, never offers;
`not-be-asked-about-the-hourly-check-nothing-would-carry` reads the band staying down when nothing
enabled would carry the warning a yes would start; and the pair around its third button —
`be-asked-again-after-putting-the-hourly-check-off` and
`not-be-asked-twice-in-a-day-about-the-hourly-check` — reads the one day of quiet a "later" buys from
both of its sides, on the deferral day the `tick deferred` premise stands up.

Everything the wait prints — the step about to be taken, and the prompt — goes to stderr, so `--json`
still leaves one machine-readable line on stdout. A driver that is not a person keeps its side open
through a pipe:

```sh
cd verification
mkfifo /tmp/go
cargo run -p amenbo-verify-gui --bin verify-gui -- scenarios/link-a-folder.yaml \
  --app ~/Applications/Amenbo.app --json < /tmp/go &
exec 3>/tmp/go   # hold the writing side open — otherwise the first echo closes it, which is the end
                 # of input, and the run stops rather than carrying on to the next step
# … stand the screen where the step just handed over says (the screen tool), then release its shot:
echo >&3
# … and when the last step has been shot, let it go:
exec 3>&-
```

## Scenario format

A scenario is an `id`, a human `title`, an optional `description`, an optional `given` (the world
the roads start from), and an ordered list of steps
under `steps_cli` and/or `steps_gui`. Each step is an `action` (changes state) or an
`assert` (an expected result), names the `domain` it touches (`task` / `decision` /
`comment` / `project` / `dimension` / `attachment` / `store` / `folder` / `repo` / `plugin` /
`mcp` / `tick`) and an
`op`, and carries named args under `with`. An action may bind its result with `as:`, and a later step
refers back to it with `target:` — an op that joins two objects names the second under its own key
(`decision link`'s `task:`), and every such key is checked back to an earlier binding, not just
`target:`.

The last four are not things filed in a store: `store` is this device's Amenbo itself — its
settings, the identity it answers `whoami` with, the build in place, and the store as a whole
(`export`, `backup`, `restore`, the integrity reads) — `folder` is a directory and the project its
`.amenbo` names, `repo` is the folder the run works in as a place with files and a git history, and
`plugin` is what is installed on the machine, whose gate is open, and what the execution log kept.

Not every object is reached by a binding. A **dimension** travels as the words a person says — its
axis and value are named in `with` (`dimension: <axis name>`, `value: <value name>`), which is what
the command takes too; a bare number there would be read as a name, not an id. Either word may also
be the **key** the row answers to, since that is what the command resolves before it
tries a name — a road writes one where being typed from outside is the point, and the name
everywhere else. The key itself is named at birth by `slug:` on `dimension create` / `value-add`,
renamed afterwards by `dimension rekey`, and read back by the `key` assert. A **folder** travels
as a plain name too (`dir: shared`), and for a different reason: a binding is answered by where a
folder sits, so the driver is the one that places it — clear of the run's own bound CWD, which a
pointer search would otherwise walk up into. One kind of folder name is answered without being
placed: a folder a road **moved** is named again afterwards — `folder rebind`'s `moved:`, `folder
vanished`'s `gone:`, `folder repointed`'s `previously:` — to say which binding is meant, and placing
it would put back the very path the move took away, which is the whole state those steps are about.
Those names are answered from what the run moved instead. A folder name also travels outside the
`folder` domain: `task update`'s `at:` and `task worked-in`'s `dir:` name which of a project's folders
a task is worked in, and are placed and read the same way the `folder` steps' are. A **plugin** is
named the way the catalog names it (`name: worktree`), which is what every one of its commands takes.

`plugin run` is the one place where a step's arguments are not Amenbo's. Everything after the
plugin's name belongs to the plugin, so `command:` is the word its own face takes, `task:` hands it
the id of a task an earlier step created, and `args:` carries anything else through verbatim. The
value that comes back is read by the `returned` assert, which has to **follow its call**: a command
face's return value is its own stdout and is deliberately not written to the execution log, so
nothing else can go and fetch it afterwards.

A **`store` action that writes a file** binds it through the same `as:` an object is bound by, and
what the name then holds is the file: `restore` names the archive it puts back the way any step names
an earlier result, so a mistyped name is a lint failure and not a driver hunting for a file nobody
wrote. The files land in the run's own throwaway space and go with it.

A **`store` action that reads a number** binds the number itself, which is the third thing a name can
hold. `sync-version` is the one: what a carrier watches is a value, not a row and not a file, and the
only thing a later step can say about it is that it moved or did not (`store version`'s `since:` and
`moved:`). Which of the three maps a name lands in follows from the op that bound it, as it does for
the other two.

One domain is not in the store at all. **`repo`** is the folder the run works in: `write-file` puts
a file there (what an attachment ingests, what the lint is pointed at), `copy-fixture` puts one
there from `fixtures/`, and `git-init` makes the folder a git repository, which is the only way the
hook slots are real enough to write into. `wire-ai` is the same kind of stand-in one tier up: Amenbo
hands over the text that starts a folder's AI on it and writes no settings file itself, so the road
past that point exists only if someone pastes — and it pastes what the build under test handed over,
into the file that build named. All of it stays inside the run's own throwaway folder — a
path that is absolute, or that climbs out with `..`, is refused.

One more is not in the store either, and is not a command at all. **`mcp`** is Amenbo reached over a
protocol rather than typed at: `serve` starts one server for one folder and holds it up
for the rest of the road, `call` calls a tool on it with the words a caller sends, and `offers` and
`answered` read what it published and what came back. It is a domain of its own because a protocol
has a conversation where a command has an exit code — the server outlives the step that stood it, and
a tool that ran and refused comes back as a *result* marked in error rather than as a transport fault,
which is what an assert there is written against.

And one is nobody typing at all — on its command side. **`tick`** is the machine's own scheduler
starting Amenbo once an hour, and what Amenbo works out once it is awake: `woken` carries out one
turn and judges what came back, which is the whole of what a scheduler ever gets — a tick leaves a
day mark and nothing else a reader can ask for, so being woken *is* the reading. `holds` reads what
the run *did* to the registration — `changed: false` being "left the machine as it was found".
**Nothing there writes one.** A registration lands outside the throwaway store a run makes, in the
launchd, systemd or Task Scheduler of whichever machine the gate is running on, which is also why
the reading is a difference and not an absolute: the store's isolation does not reach the scheduler,
so a road asking whether a registration is held at all would read the one belonging to whoever is
running the gate. A road that installed one from the terminal and left it would leave an hourly
timer on a release box — `be-offered-a-start-at-login` draws the same line for the login
registration, and that half is walked on the real machines.

The consent in front of that wake is the one part of the tick a person meets, and it is on a
screen — so these are screen roads alone, the terminal's way in (`tick install`) asking
nothing. `banner` reads whether the band offering the hourly check is standing across the app, up
only while the device is unanswered, a dated task is open and a `task.due` subscriber is enabled
somewhere; `banner-answer` gives one of its three answers (`start` / `never` / `later`); `setting`
and `set` read and move the row in Amenbo's own settings that holds the answer afterwards — the one
way a no is taken back. A road that answers `start`, or moves the row to `on`, registers the timer
for real, so it takes that back (the row to `off`) before it ends, and what it asserts in between
stays on the answer's side of the line `holds` draws: the answer having landed, never the machine's
registration as an absolute. The last piece is a premise rather than a road: `deferred` stands up
the day the band was last put off, because "later"'s whole meaning — quiet today, back tomorrow —
spans two launches, and no single run holds both.

A few ops exist to put something **wrong**, because a repair cannot be shown working over a store
where there is nothing to repair — and a sweep that sweeps nothing looks exactly like one that works.
`folder legacy-pointer` leaves a bound folder's `.amenbo` in the shape an older build wrote, which is
what `store doctor-fix` puts right. `folder foreign-pointer` leaves one claimed by another store,
which is what the guard in front of every read refuses — a build stamps its own name as it writes, so
the one store that cannot leave another's pointer is the build under test. It is the run's own
pointer with a different name on it, which is the fixture worth making: every other field agrees, and
a pointer whose id and slug both check out is the one nothing but the name can turn away.
`plugin stale-manifest` leaves an installed plugin recording a
build the catalog has moved past, which is what `plugin update` puts right — the catalog publishes one
build, and an asset is trusted only by the key of the catalog that served it, so there is no second
build to install first and no way to sign one into existence. Three of the `plugin declare-…` ops put a setting into what an
installed plugin says it takes: what a plugin takes is the author's word, Amenbo never invents a field,
and **no plugin in the official catalog declares one at all** — so every road through `plugin config`
would go unwalked until one does. `declare-setting` writes the plain kind, the line a reader types and
reads back; `declare-secret` writes the flag that sends a value down the other road, which fails
silently and in plain text; `declare-choice` writes a setting whose answers the author listed, and the
default that stands until someone gives one, which is what keeps a choice made, a choice declined and a
question nobody has answered apart. Any of the three also takes `translated:` — the words that field
carries in the author's other languages, keyed by language code: the `label` a form draws it under, and
for a choice the `options` its candidates are, keyed by the value each one stores. They land where an
install puts what a catalog published, beside the manifest rather than in it, which is what a form reads
and why one follows a reader changing language with nothing fetched. No published plugin declares a
setting, so none has one translated either — both halves are out of reach for the one reason, and are
written by the one door. Any of the three takes `required: true`, the flag that says the
plugin cannot work without an answer — the fail-closed enable is refused while the crossing holds none,
and no published plugin declares that either. `declare-action` is the fourth of that family and writes what the
author offers to *do* from that same form — the button, the one value it asks for at the press, and whether
that value is one the author called a credential — which
no published plugin declares either, so the operation is a face no install reaches; `plugin press-program`
stands the program behind it in, since an operation is code being run and what the form draws is one line
of what that code said. `declare-check` writes the other half of that block — the judgement an author has
raised on the values before a gate opens on them — and `plugin check-program` answers it, `ok:` being the
scenario's to choose, since what a road about a gate wants is the same values turned away and then let
through. A plugin has one program, so those two stand-ins replace whatever stood there before, each other
included — but they do not need each other: `press-program` answers a check with a yes on the stream a
press never reads, so a settings block carrying both halves is walked by standing in that one, and
`check-program` is what a road reaches for when the verdict itself is under test. `press-program` also
takes `writes:` and `writes_value:`, which leave the press storing one of the plugin's own settings back
through `plugin config set` — the door a plugin's own value arrives by, and the only one a field its
author marked `readonly` has. A road that names neither gets the program as it was, writing nothing. `plugin declare-scope` writes the layer the author
declared — one project's rows, or the device's — which is the same kind of word and unreachable for the
same reason: a manifest saying nothing means `project`, and every published plugin says nothing, so the
road a machine-wide plugin walks (one enable, one window on the whole device) exists only once this is
written. `plugin slow-program` leaves an installed plugin
taking seconds to answer, which is the only way a queue holds anything to read: a row leaves the moment
its plugin replies, so the backlog `plugin log` reports is the window a slow plugin holds open, and
every plugin the catalog publishes answers in the time a process takes to start. `plugin echo-program`
leaves one answering with the config it was handed, which is the only witness a setting's delivery has:
it travels on the child process — as an environment variable for a secret, in the stdin document for
everything else — and the published plugins use their settings rather than report them.
`plugin read-back-program` leaves one calling Amenbo back, which is the only witness the read-back
route has: an event names a record and carries none of it, so the content is fetched by running the
binary with the store and the window Amenbo handed over — and the published plugins work everything
out from the repository they are called in, asking Amenbo nothing. `plugin unbadge` takes the catalog's badge off an installed
plugin, which is the only way a road meets a stranger's: the badge is the catalog's to grant and an
author who could write it onto themselves would be the reason it is worth nothing, so every plugin
the official catalog serves arrives with it and no install reaches the state a user is in the moment
they install from anywhere else. `plugin installed-dir` shuts what
is installed away and gives it back, which is the only way a write's delivery is left standing:
delivery rides along with the write that caused it, so a push made by hand carries something only
where that drive never ran — and Amenbo skips it exactly when the installed plugins will not read.
They are the
same idea as `repo write-file`: the
state on disk a scenario has to arrive at, and cannot reach by using Amenbo, the driver makes. Reach
for one only when the line under test is what Amenbo does about that state.

One of them puts nothing wrong — it puts time. `store worn-in` leaves the store reading as one
somebody has been coming back to: the launches this device has tallied, and the days its records were
written on. Both are what Amenbo holds an unasked offer behind, and neither is reachable by doing
anything — a launch tally is raised by the app coming up, and days are days, so a run that tried to
earn this would have to last a week. It is written straight onto the store, which is the one place a
driver reaches past the binary's face: the two scalars Amenbo tallies into, and one record moved back
per day asked for (the store has to hold at least that many). A store is a plain SQLite file — no
shipped path keys one — so this crate carries `rusqlite` for that single op and for nothing else.

`fixtures/` is for what a scenario cannot hold itself. This tree's prose rule keeps a bare Amenbo
reference out of every `.yaml`, and the lint has nothing to find unless a file really carries one —
so the file carries it and the scenario names the file. Bytes that are not text at all are the other
half of the same shelf: an image a screen road has an operator choose in a picker is a picture being
drawn, so any file of the right name would prove nothing.

```yaml
id: delegate-to-ai
title: Work handed to me-ai reaches the next session's mailbox and is held by one session alone
steps_cli:
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
  - { type: action, domain: task, op: assign, with: { target: seed, assignee: me-ai } }
  - { type: assert, domain: task, op: listed, with: { filter: "assignee:me-ai status:todo", target: seed, present: true } }
steps_gui:
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
  - { type: action, domain: task, op: assign, with: { target: seed, assignee: me-ai } }
  - { type: assert, domain: task, op: listed, with: { filter: "assignee:me-ai status:todo", target: seed, present: true } }
```

The op vocabulary is a **closed registry** in `core/src/lib.rs`: an unknown op is rejected,
so a typo never runs as a no-op. Drivers grow the registry (and their own op → driver
mapping) as new ops are needed. Each op declares the args it takes as words, and the lint
checks the value arrived as one: YAML types an unquoted scalar by its shape, so a SHA of
nothing but digits parses as a number, and a driver would only meet it at the far end of a run.

**Write sample values in the shape of the real thing** — a SHA that looks like a SHA, a title a
person would type. An extreme value (an empty string, a single character, something enormous)
belongs in a scenario only when the extreme is what the line is about, and then say so on the spot:
nothing else can tell a value chosen on purpose from one chosen carelessly.

A `field` assert names its value by a dotted path into the read it is about — an object's `show
--json`, or one of the reads the store answers about itself (`store`'s `config` / `identity` /
`update`) — so what the output nests is reachable without a new op per corner:
`placement.project.name` walks two objects,
`blocked_by.0.name` indexes an array on the way, and a path that runs off the output is a mismatch
rather than an error. The `store doctor` assert reads its verdict through `ok`, and takes an `issue`
— a kind out of doctor's own list — when what is under test is a single problem appearing or going:
most of what doctor raises is a warning, and a warning leaves `ok` alone. A `store snapshot` assert
takes `absent: <text>` when what is under test is something that must **not** have left the store: what
Amenbo handed out is read as bytes — one file for a backup, the whole folder for an export — and the
word is looked for verbatim, which needs no reading of the layout around it. A `plugin config` assert
takes `secret: true` for the same kind of question one tier up: the read says the setting is a secret,
and does not hand the value over with it. It takes `state:` for the question a value cannot answer for
itself — `chosen`, `none` or `unanswered` — since a choice answered with none of its candidates and one
nobody has answered both hold no chosen value, and only the second follows the author's default.
And it takes `holds:` on a screen road for the plainest reading of all — a typed line standing in the box a
form draws it in, which is what a road asks after something that could have taken it away. That one is the
screen's alone, `equals` being how the same value is read where there is no box.
Both it and the `plugin config-set` beside it take `project:`, the crossing the value is held at — a
setting belongs to one, and a terminal says which by standing in a folder bound to it, so the driver
stands in that project's folder before it types. Naming none is the folder the run itself works from,
which answers to the project Amenbo raised for it; a premise that leaves it out while the road reads
the value at a crossing of its own writes somewhere neither road then looks. That word is the
terminal's alone — on screen the row the settings are opened inside has already answered it, and the
GUI harness turns it away — and it travels as a word rather than as a binding, so a test over the
scenario set holds it to a project something raised, the way the one that opens a project is held.
A `dimension listed` assert takes `side: task` / `decision` for a question its plain form cannot put:
not whether the axis is defined but whether that side is offered it at all, which an axis narrowed with
`dimension applies-to` stops being while staying on every listing. It takes `target:`
beside it for the face with no listing to read the offer off — a screen reads the offer as the control a
record's own pane keeps per axis, so the road names the record whose pane is opened.
Its neighbour `filter-refused` asks the other thing a filter can come back with: not who is in the
listing but that there is no listing, the question having been turned away — which is what a `dim:`
written on the side its axis does not classify meets. It is an assert of its own rather than a
`refused:` on `listed`, since `refused:` is an action's word and an empty page and a refusal are
exactly the two answers this separates; `code:` names the error code the refusal has to carry.
A `listed` assert asks whether the task is in the listing; give it
`position: first` / `last` instead of `present:` when what is under test is the order the store
keeps, which is the only place a reorder is visible. Its neighbour `narrowed` asks the question a
screen puts instead: which of the cards drawn a moment ago are drawn still. It names no filter because
there is none to name — the narrowing is the screen's own, and what did it belongs to the move in
front of the assert. That is either of the two narrowings a board has, and one assert answers for
both: `narrow` types words over the columns, which travel as words and are matched over the whole
record including faces a card does not show, and `open-filters` / `choose-filter` / `close-filters`
walk the values on its axes — each press adding to the set that axis is narrowed to, each pair
written as the CLI writes it, since the chips carrying them are in the reader's own language.
`filters-folded` reads what the fold leaves: the values off the screen, and the count of narrowing
axes on the control they folded into.

The decisions tab has that same panel over its own list — and a box of its own beside it — and its own
entries for both: `decision narrow` / `open-filters` / `choose-filter` / `close-filters` /
`filters-folded` / `narrowed`. They are
separate from the board's rather than shared with them because a road has to say which of the two
tabs it is standing on: a step that named neither could be walked on either and would prove whichever
the operator happened to be looking at. On the decision side `narrowed` is the screen's answer to what
`listed` asks the terminal — the terminal carries the whole narrowing as one `--filter` line, where
the screen composes it press by press and reads what is left.

### `given:` — the world a road starts from

Some roads stand on records the road itself never makes: a plugin already installed, a catalog
already registered, a project that is simply there. Left unwritten, that is a precondition living in
whoever prepared the screen last — an operator reading the file cannot tell what to put in place,
and no driver can put it there for them.

`given:` is where it is written. It carries **actions**, in the same shape a road's steps have, and
what it names with `as:` is in scope on **both** roads — the world belongs to the scenario, not to
one of its roads:

```yaml
given:
  - { type: action, domain: plugin, op: install, with: { name: worktree } }
  - { type: action, domain: task, op: create, with: { title: SEED }, as: seed }
steps_gui:
  - { type: assert, domain: task, op: listed, with: { filter: "status:todo", target: seed, present: true } }
```

The driver stands it up before it walks, and the line it may not cross is the screen's own moves: an
op is allowed here only if it is one a driver can reach without the screen (a second closed list in
`core/src/lib.rs`, cut out of the registry). Opening a card, answering the question it puts, typing
words over a listing — those are what a screen road is watching, and a premise that carried them out
would leave the road proving something already done. For the same reason a premise takes no
`assert` (nothing is being proved yet) and no `refused:` (a refusal leaves nothing standing).

**A screen road needs one even where the records look like its own to make.** A board with nothing on
it draws the first loop in place of its columns, and the way to add a card is in a column head — so a
road that opens by filing a task has no way to make that move, and an untouched store has no project
to open a board for in the first place. The premise is what puts the columns there: the work the road
takes for granted, or — where filing is what the road is about — one piece of ordinary work that was
already standing on that board.

**A road that opens a project names one that is there**: a project the world raised, or `cwd` — the
project a run already stands in, which Amenbo raises for the folder the run works in and calls after
it. The name travels as a word rather than as a binding, so nothing in the lint or the render would
catch a road sending an operator to hunt a list for a project nobody made; a test over the scenario
set holds it instead.

A board the road raised itself is that same empty board, and no earlier step can fill it: a project a
road makes is one the premise never saw. So a second project a road only reads across — the neighbour
a narrowing has to have something to leave out — is stood up in the premise whole, with the records
in it, and the road is left holding the narrowing alone.

The same goes for anything a screen road hands to Amenbo from outside it. Hanging a file is a move on
a record and not a way of making one, so the bytes have to be lying in the run's folder before the
step that attaches them: `repo write-file` is how they get there, and a road that asks for a file
nobody wrote stalls on the operator having nothing to pick.

A premise is not a road: a file carrying a world and no steps is refused like any other file nothing
walks.

The list holds two premises that are not records at all, and both stand up the passage of time —
the one world a road can only be given, every other premise being something somebody could have done
a moment before the run. `store worn-in` is how much Amenbo has been used on this device, which they
could only have done over days; `tick deferred` is a day having passed — or not — since the band was
put off, which no run can wait out, the band being judged once at launch.

`folder foreign-pointer` is on the list for the neighbouring reason: not that nobody could have done
it a moment before, but that nobody under test could have done it at all. A build stamps its own name
as it writes a pointer, so the one store that cannot leave another's is the one being driven, and on
screen there is not even a command to try it with. Both roads that read the claim — the terminal's
refusal, the row the screen lists the folder as — therefore open on it.

**A premise that does not stand ends that scenario, red, on the line that failed, and the road is not
walked at all.** Judging a road against a world half built says nothing about the road — every line it
then wrote, passing or failing, would be about the wrong thing. It is that scenario's failure and not
the run's, so a set keeps going and the report names the premise as what broke (its lines are numbered
in their own sequence, so a premise and a step never both call themselves the first).

### `refused:` — the step that is right to fail

Some of what Amenbo promises is a **refusal**: a reserve of a task another session holds comes back
`already_reserved`, one whose premises are unmet comes back `not_ready`, and a write outside an AI's
reach comes back `out_of_reach`. A driver that reads every non-zero exit as its own failure cannot
write that line down at all — so an action names the code it expects to be turned away with:

```yaml
- { type: action, domain: task, op: status, with: { target: held, status: in_progress, refused: already_reserved } }
```

The op and its args are the ordinary ones; what is under test is the guard standing in front of
them. The step is then judged like an assert: refused with that code passes, **going through
fails** — that is the regression it exists to catch — and being refused for some *other* reason
fails too, since a different guard is not the one the line is about. A refused operation produces
nothing, so it takes no `as:`.

A screen has no exit status to compare, so on a `steps_gui` road the word changes the **instruction**: it
tells the operator that being turned away is the step going right rather than their own hand going wrong,
and the shot they leave is the screen carrying the refusal. Which guard refused is then read by the assert
after it — a screen offers a sentence, never a code.

### `steps_cli` / `steps_gui` — one goal, a road apiece

**Having steps is what says a driver runs this line.** There is nothing else to declare, so nothing
that can disagree with the steps: the CLI runner skips a scenario with no `steps_cli` road, and the
GUI harness refuses one with no `steps_gui` road rather than spending an eye on `Review` steps
nobody sent there. A file with neither is refused by the lint — a scenario nothing walks rots while
the set around it reports green.

Write the road the driver actually travels. A screen's road is a different shape, not a rendering
of the CLI's: linking a folder to a project is one command to type, and on screen it is a card to
open, a project to answer for and a folder to pick. Written once for both, one of the two comes out
bent. Where the two roads really are the same ground, write it in both — the duplication is small
next to a file where the screen's half quietly went stale.

Some ops exist for one road only, and the registry carries them all the same: opening a card and
answering which project it asks for are moves a screen's road is made of and a terminal's has none
of, so they are written in `steps_gui` and the CLI driver maps neither. A driver maps the ops it
meets; one it does not meet is not its to map. Cutting that board along another axis (`project
group-by`) is the same kind, and so is reading one of the columns back (`project column`): what a
terminal answers with is a listing, and a listing has no columns
to recut — the axis is a word in the filter there, not a way the answer is laid out. Answering the question an opened project puts, choosing
which tool the text is for, pressing the button that hands it over, and dropping the answer again
(`repo ai-launch-consent` / `ai-launch-pick` / `ai-launch-copy` / `ai-launch-consent-clear`), are the
same kind: a terminal asks inline and prints the text where it stands, so there is nothing there to
answer, to choose between, or to press — and the answer it writes it never reads back, so it has no
face to clear it from either. The same three moves on the project's own settings, where the way to the
text stays open after the wiring lands (`repo ai-launch-request` / `ai-launch-request-pick` /
`ai-launch-request-copy`), are ops apart from those rather than the same ones read elsewhere: what
those name is the report, and where these are walked there is none — so an instruction sending the
operator to it would be one nobody could carry out. Typing words over a listing already drawn and reading which cards they
left (`task narrow` / `task narrowed`) is the same kind: a terminal has no listing standing in front of
it, so asking a word where it is written is one command there and there is nothing to narrow. Pressing a
hit through to the record it points at, and reading which record that opened (`task open-hit` /
`task opened`), is the same kind again: a terminal prints its hits as text, and the ref is typed into
`show` rather than pressed. Reading the cross-cutting search's narrowing box as one that cannot be typed
into at all (`task narrowing-shut`), the button a consent is taken back with as one that cannot be
pressed (`repo ai-launch-consent-clear-shut`), and the button an app's row hands its road over with while
nothing is ticked on it (`repo mcp-road-shut`), are the last of them: a flag either arrives on a command
line or it does not, so only a screen holds a control a reader can see, reach and not use — which is the
shape the box takes while no side is chosen and there is no grammar to read the words in, the shape the
button takes while there is no answer left to clear, and the shape either road's button takes while the
row has no folder to hand over. All three are read off how the control is drawn, not off what it does
when used: one shut in the build and painted like a live one is one a reader still reaches for.

Pressing a smart view open from the sidebar, reading the warning its row carries before that press, and
reading which tasks the listing behind it holds (`task open-view` / `task view-warns` / `task
view-lists`), are one road's for a reason of their own: what they are about is being told without asking.
A terminal is only ever asked. It answers the same question about days when a reader types for it, which
is what `task status-bucket` reads — but there is no row standing in front of it to carry a colour, and
nothing to press through to a listing it has already printed.

Opening the fold that offers the other way in, and reading an app's row behind it (`repo mcp-open` /
`mcp-app` / `mcp-road` / `mcp-road-shut`), are one road's for a reason of their own: what the fold is for
is the reader whose AI cannot open a folder at all, and a terminal standing in that folder is the reader
who never needed it.
There is nothing folded there, no list of apps, and no row for one to be read off — the two faces that
draw them are the screen's, and a terminal that grew one would be a road of its own rather than this one
written out again.

`repo mcp-in-app` walks off that screen and is one road's for a further reason: what it reads is another
program, opened by hand with the file the fold just handed over. It is written for the one app whose
settings Amenbo writes itself, with nobody in between — everywhere else a reader's own AI does the edit,
and what Amenbo owns there is the wording, which is held up without leaving this workspace. The step is a
`Review` whichever way it goes, since the shot a run takes is of the build under test and the window that
settles this one is not; the instruction asks the attending AI for that picture instead. It is walked once
a release, and going further — the app's own AI making a task through the server — is a round trip walked
once and not per release, what is on the far side of it being somebody else's product.

Reading what a plugin's own check said about its settings (`plugin checked`) is one road's for a reason of
its own: the verdict's sentences are the author's and are drawn on the settings form, while a terminal
meets the same refusal as an error code on the enable it turned away — which is what the `refused:` on that
step already reads.

Pressing an operation a plugin's author put on its settings form, answering what that press asks for, and
reading the line it left, the box it asked in and the button before the gate was open (`plugin press` /
`press-answer` / `press-said` / `press-asks` / `press-shut`), are one road's for a reason of their own: a
terminal reaches the same author's code through `plugin run`, which names the call itself, takes whatever
arguments are typed after it, and answers with a return value — so what is under test here, a press
choosing among the calls a manifest declared and asking for what that one needs, has no terminal to walk.

Changing the language the interface is read in, and reading the words a plugin's own text reaches a
reader as (`store set-language` / `plugin line` / `plugin asks`), are one road's for a different reason:
the setting is reachable from a terminal, but nothing a terminal prints is drawn in it. What the CLI
answers is English whatever the setting says, so a road that changed it there would be moving a value
nothing it could then read depends on — and the sentences this is about are drawn in one place. `line`
is the one under a market row's name, `asks` the one a settings form draws a field, or one of a choice's
answers, under.

Giving a project the image it shows for itself, taking that image away, and reading which of the two
the square holds (`project set-icon` / `clear-icon` / `icon`), are one road's for the plainest reason
of all: `project update` is the terminal's whole door onto these fields and it takes a name, a note, a
colour and a view. There is nowhere on it to hand over an image, so a road written for the CLI would
be naming a command that does not exist. The reading is a `Review` on both of its states — what is on
the shot is a picture, and neither the image nor the colour-and-letter a project falls back to puts a
word there. `file:` here is not `attach`'s: what is under test is the picture, so the road names a
file a premise copied off the fixtures shelf rather than one the operator brings, and the run says
where it landed before the first step is handed over.

One road's alone is sometimes an *argument* rather than a whole op, and then it is the driver that
cannot answer it which says so. A search hit is the case: what the row **calls** the place it points
at (`landed_on`) and the run of characters it **marks** inside its excerpt (`marked`) are drawn, not
reported — down the CLI's pipe the first arrives as a `kind` and a comment ref for the reader to put
together, and the second as a pair of offsets. Both are declared on `found` like any other argument,
the GUI harness renders them into the line an eye closes, and the CLI driver refuses a step naming
either instead of passing over it. Silence would be worse than a red: a step that asked and was never
answered reads exactly like one that was.

**A number is a query no scenario can write.** The store issues it, so a road knows which record it
means and never what that record was numbered — which is why the three steps that put something into a
search box (`task found` / `decision found`, `task narrow`, `task open-hit`) take `number_of:` naming a
binding in place of `words:`, and exactly one of the two. `spelled:` is the shape it goes in as, `bare`
(`12`) or `hash` (`#12`), both of which the box reads. The two roads fill it in differently and for the
same reason the rest of this section names: the CLI driver knows what the run created and substitutes
the number, while the GUI harness renders its lines from the YAML alone — before any world stands up —
so it names the record instead and the operator reads the number off the screen. Which box was typed
into is what settles the side a number with no type code is read as: `task narrow` types over the
columns and answers with a task, `decision narrow` types over the rows on the other tab and answers
with a decision.

What such a query is answered with is a record put at the **top** of the answer, ahead of whatever the
words matched, so `first: true` on `found` asks for that place rather than for a place anywhere in it.
The CLI driver reads the order off the hits; the GUI harness leaves it as a `Review`, a reading giving
back which words are on a shot and never which line they were on.

The words below that top row are reached the same way round. `mentions:` on `task update` and `task
comment` names a record whose number is written into the text the step writes, after the words it
wrote — the only way a road can put one record's number inside another's, since the store issues it.
That is what lets one answer hold both halves: the record the number names at the top, and the record
that merely wrote it down underneath.

An argument can also mean two different things on the two roads, and `attach`'s `file:` is the one
that does. To the CLI driver it is a path in the run's own folder, put there by a `repo write-file` a
step earlier; on screen it is a **name** and nothing more — both ways in there (the picker and the
drop) read the disk the operator is sitting at, and nothing a run lays down is anywhere either of them
is pointed, so the instruction asks for a file of that name and the operator brings one. Nothing is
given up by that: what a search reaches of an attachment is what it is called, never its bytes. A
`url:` has no way in on that face at all, so the GUI harness refuses one rather than writing a line
about a face the app does not have.

Bindings belong to the road they are made on. A `target:` in `steps_gui` resolves against
`steps_gui` alone, because the two lists are never walked in one run.

Which roads a file carries is a cost question. A CLI line costs a process and an exit code, so that
set aims wide; a GUI line costs a screenshot, an OCR pass and a human reading the `field` asserts
OCR cannot judge, so that set stays a chosen few.

## Lint

The loader checks both layers — the YAML parses into the typed model (misspelled keys are
caught), and the semantic pass (known ops, required args, each arg of the type its op takes,
every reference resolving to an earlier `as:` on the same road). Run it over the whole scenario
set:

```sh
cd verification && cargo run -p amenbo-scenario --bin lint
# or against specific files:
cargo run -p amenbo-scenario --bin lint -- scenarios/delegate-to-ai.yaml
```

Non-zero exit on any parse or validation failure, so it drops into a make target or CI.
The crate's tests assert that every real scenario lints and every invalid fixture is
rejected:

```sh
cd verification && cargo test
```
