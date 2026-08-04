# The README's CLI demo, as a script: `vhs assets/cli-demo.tape` re-records assets/cli-demo.gif
# from the repository root. It runs the CLI this tree builds, so re-record it after a change to
# what the demo shows:
#
#     cargo build -p amenbo-cli && vhs assets/cli-demo.tape
#     rm assets/cli-demo.transcript.txt && cargo test -p amenbo-cli --features e2e --test demo_tape
#
# The second line is not optional. A gif carries no diff, so what watches this film for rot is
# assets/cli-demo.transcript.txt: the lines below, replayed and compared by tests/demo_tape.rs
# (deleting it is how it is rewritten). Left behind, it either greens a stale picture or reddens a
# fresh one.
#
# The recording is a throwaway store, and the two things that keep it one are the same two the
# `make verify` script uses: AMENBO_HOME pointing at a scratch directory (the only thing that keeps
# a run out of the real app-data tree), and a working directory with no `.amenbo` above it. The
# setup and the teardown are wrapped in Hide/Show, so the film shows the demo and nothing else.
#
# The scratch paths are fixed rather than mktemp so that what the terminal prints is the same on
# every machine, and every command declares its own facet with `--actor`, so each half records as the
# facet it means to whatever the recording developer's shell exports.

Output assets/cli-demo.gif

Set Shell "bash"
Set FontSize 20
Set Width 1280
Set Height 620
Set Padding 24
Set WindowBar Colorful
Set TypingSpeed 35ms
Set Framerate 24

Hide
Type "export PATH=$PWD/target/debug:$PATH" Enter
Type "export AMENBO_HOME=/tmp/amenbo-demo-home PS1='$ '" Enter
Type "rm -rf /tmp/amenbo-demo /tmp/amenbo-demo-home; mkdir -p /tmp/amenbo-demo/website" Enter
Type "cd /tmp/amenbo-demo/website; clear" Enter
Show

Sleep 1s

# The human's half: a folder becomes a project, and work is filed in it.
Type "# a folder becomes a project — the store is one SQLite file on this machine" Sleep 400ms Enter
Sleep 800ms
Type "amenbo init --name Alex --actor human" Sleep 400ms Enter
Sleep 2.5s

# Off camera: `init` leaves one session-start question for the next human command, and a terminal is
# where it fires. A film cannot answer it in character — and left unanswered it eats the next line the
# tape types, which is how a re-recording silently films a broken tour. So it is answered here, and the
# screen reset, which also keeps the half below from scrolling. `tests/demo_tape.rs` skips Hide blocks,
# so this run stays out of the transcript.
Hide
Type "amenbo task list --actor human" Enter
Sleep 800ms
Type "n" Enter
Sleep 800ms
Type "clear" Enter
Sleep 500ms
Show

Type "amenbo task add --title 'Draft the launch note' --project website --due +3d --actor human" Sleep 400ms Enter
Sleep 1.5s
Type "amenbo task add --title 'Ship the installer' --project website --to me-ai --actor human" Sleep 400ms Enter
Sleep 1.5s

# What add returns is still being created — nobody can take it until the writing is finished.
Type "amenbo task finish-creating 1 --actor human" Sleep 400ms Enter
Sleep 1.2s
Type "amenbo task finish-creating 2 --actor human" Sleep 400ms Enter
Sleep 1.2s
Type "amenbo task list --actor human" Sleep 400ms Enter
Sleep 3s

Type "clear" Enter
Sleep 500ms

# The AI's half: the same store, its own mailbox, and the spec it works from.
Type "# the AI works the same store — this is its mailbox" Sleep 400ms Enter
Sleep 800ms
Type "amenbo task list --filter 'assignee:me-ai status:todo ready:yes' --actor ai" Sleep 400ms Enter
Sleep 2.5s

Type "amenbo task status 2 in_progress --actor ai" Sleep 400ms Enter
Sleep 2s

Type "# and the CLI hands the AI the spec for what it is about to run" Sleep 400ms Enter
Sleep 800ms
Type "amenbo agent --command 'task done'" Sleep 400ms Enter
Sleep 3.5s

Type "amenbo task done 2 --actor ai" Sleep 400ms Enter
Sleep 3s

Hide
Type "rm -rf /tmp/amenbo-demo /tmp/amenbo-demo-home" Enter
Show
