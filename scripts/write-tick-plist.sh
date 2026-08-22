#!/usr/bin/env bash
# write-tick-plist.sh — write the hourly tick's launchd agent for the build about to be made.
#
# The plist is carried inside the app bundle and registered from there through SMAppService, so
# macOS lists the row under amenbo rather than under the developer whose certificate signed it.
# Bundled means signed, which is why this is written before the bundle rather than at run time.
#
# **It is written per build because the label is what macOS keys the record by.** One user has one
# record per label, so production and a dev build sharing a label are not two background items but
# one, resolving to whichever registered last: a dev app that inherits a dead instance's record
# answers "registered" while nothing can ever fire. The label carries the channel for
# the same reason the Linux units and the Windows task do
# (crates/amenbo-core/src/tick.rs::registration_name), and the file is named for the label because
# that is the convention SMAppService is used under.
#
# The CLI it runs is named per build for the same reason: the bundle carries the sidecar under this
# build's own name (`Paths::command_name`), so production's `Contents/MacOS/amenbo` and a theme
# preview's `Contents/MacOS/amenbo-dev-<id>` are different files, and a plist naming the wrong one
# is a timer that registers and never runs.
#
# The Makefile owns both names and hands tauri the entry that bundles the file; this writes exactly
# one file and says where it went.
#
# Usage: scripts/write-tick-plist.sh <label> <cli-name>
#        (e.g. work.amenbo.tick amenbo, work.amenbo.tick.amenbo-dev amenbo-dev)
# Exit codes: 0 = written, 1 = a name is missing.
set -euo pipefail

label=${1:-}
if [ -z "$label" ]; then
    echo "✗ tick plist: no label given (e.g. work.amenbo.tick)" >&2
    exit 1
fi

cli=${2:-}
if [ -z "$cli" ]; then
    echo "✗ tick plist: no CLI name given (e.g. amenbo) — it is what the wake-up runs" >&2
    exit 1
fi

root=$(cd "$(dirname "$0")/.." && pwd)
dir=$root/app/src-tauri/launchd
mkdir -p "$dir"
out=$dir/$label.plist

cat > "$out" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<!-- Written by scripts/write-tick-plist.sh as this bundle was built. The hourly tick,
	     registered through SMAppService from the app bundle this file is carried in, so macOS lists
	     it under amenbo's own name rather than the developer's. Bundled means signed, so nothing
	     here can be rewritten at run time — which is harmless for a timer whose whole design is one
	     plain hourly wake-up. -->

	<!-- The label carries this build's channel, and macOS keys the record of a background item by
	     it: two builds under one label share one record and fight over it. -->
	<key>Label</key>
	<string>$label</string>

	<!-- What the wake-up runs: the CLI carried in this same bundle, named relative to the bundle
	     root, so the job follows the app wherever it is installed. -->
	<key>BundleProgram</key>
	<string>Contents/MacOS/$cli</string>
	<key>ProgramArguments</key>
	<array>
		<string>Contents/MacOS/$cli</string>
		<string>tick</string>
		<string>run</string>
	</array>

	<!-- On the hour, every hour. StartCalendarInterval and not StartInterval: an interval drops the
	     turns that came round while the machine was asleep, and a calendar entry runs the missed one
	     once on wake — measured on all three schedulers before this was written. -->
	<key>StartCalendarInterval</key>
	<dict>
		<key>Minute</key>
		<integer>0</integer>
	</dict>

	<!-- Nothing is waiting for this, so it takes the background band: lower priority, throttled I/O,
	     and no claim on the CPU a person is using. -->
	<key>ProcessType</key>
	<string>Background</string>

	<!-- Registering is not a reason to run: the first turn is the next hour. -->
	<key>RunAtLoad</key>
	<false/>
</dict>
</plist>
PLIST

echo "→ ${out#"$root"/} (Label $label)"
