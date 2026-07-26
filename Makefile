# amenbo build/install — dev/prod are kept apart.
#
# Split:
#   prod  : command `amenbo`     / app-data `work.amenbo.amenbo`      … real task management (dogfood lives here too)
#   dev   : command `amenbo-dev` / app-data `work.amenbo.amenbo-dev`  … development and experiments (never touch prod data)
# The app-data name is switched by core via `AMENBO_APP_NAME` (at build time). dev builds into its
# own target dir so it does not contend with prod over rebuilds.
# The dev GUI splits once more, by AMB-T-ID: unset is the shared dev app above, AMB-T-ID=<id> is a
# throwaway instance owned by one task (app-data `work.amenbo.amenbo-dev-<id>`). See GUI_DEV_*.

CARGO_BIN := $(HOME)/.cargo/bin
APPS_DIR  := /Applications
BUNDLE_DIR := app/src-tauri/target/release/bundle/macos
GUI_APP     := $(BUNDLE_DIR)/amenbo.app

# The dev GUI comes in two shapes, and AMB-T-ID picks which one every dev-GUI target builds and
# installs. Unset is the shared dev app: one permanent bundle, the place to keep a grown setup
# (plugins, catalog, projects) that no task may delete. AMB-T-ID=<id> is a throwaway instance one
# task owns — its own bundle identifier, product name and app-data, so two parallel sessions verify
# their own work instead of installing over each other. `devtool task finish <id>` deletes the
# instance's bundle and its app-data together with the worktree.
#
# The name is amenbo's own task namespace on purpose, and not a plain word like TASK: make reads
# the environment as well as the command line, so a plain word is one a shell may already export
# and this build would silently obey. A hyphenated name is one no shell can assign at all, which
# leaves the command line as the only way in.
AMB-T-ID ?=
ifeq ($(strip $(AMB-T-ID)),)
GUI_DEV_NAME := amenbo (dev)
GUI_DEV_ID   := work.amenbo.app.dev
GUI_DEV_DATA := amenbo-dev
else
# Digits only, the same canonical task ref devtool pins its worktree and branch names to: the
# bundle name has to be the identical string on both sides, or teardown looks for a bundle that
# was never built under that name.
ifneq ($(shell printf '%s' '$(AMB-T-ID)' | tr -d '0-9'),)
$(error AMB-T-ID must be a task number (digits only) — got '$(AMB-T-ID)')
endif
GUI_DEV_NAME := amenbo (dev $(AMB-T-ID))
GUI_DEV_ID   := work.amenbo.app.dev.$(AMB-T-ID)
GUI_DEV_DATA := amenbo-dev-$(AMB-T-ID)
endif
GUI_APP_DEV := $(BUNDLE_DIR)/$(GUI_DEV_NAME).app

DIST_DIR := dist
VERSION  := $(shell awk '/\[workspace.package\]/{f=1} f&&/^version/{gsub(/[",]/,"",$$3);print $$3;exit}' Cargo.toml)

# Where `sweep-stale` bites. Only when the total exceeds this size does it drop build artifacts
# untouched for this many days. The day count is chosen so a from-scratch cache build in between
# does not cost us the assets.
SWEEP_LIMIT_GB ?= 20
SWEEP_DAYS     ?= 3

# mac GUI bundles are built for an explicit arch (Intel is a supported target, so the unified
# installer ships as two pkgs). x64 is cross-built from Apple Silicon; we always pass
# --target so the arch is stated, not inherited — which also keeps the bundle paths uniform
# (tauri puts them under target/<triple>/ whenever --target is given). Override with
# MAC_GUI_ARCH=amd64. wharfy's arch vocabulary (arm64/amd64) names the dist artifacts; tauri's
# dmg name uses its own (aarch64/x64).
MAC_GUI_ARCH   ?= arm64
$(if $(filter $(MAC_GUI_ARCH),arm64 amd64),,$(error MAC_GUI_ARCH must be arm64 or amd64 (got: $(MAC_GUI_ARCH))))
MAC_GUI_TRIPLE := $(if $(filter arm64,$(MAC_GUI_ARCH)),aarch64,x86_64)-apple-darwin
MAC_BUNDLE_DIR := app/src-tauri/target/$(MAC_GUI_TRIPLE)/release/bundle
MAC_GUI_APP    := $(MAC_BUNDLE_DIR)/macos/amenbo.app
# GUI dmg: tauri names it `<productName>_<version>_<arch>.dmg` (prod productName=amenbo).
# We copy it to a stable name under dist/ as a non-installer supplement (NOT a wharfy
# bundle — the mac release bundle is the unified .pkg, see dist-gui-mac).
GUI_DMG_SRC  := $(MAC_BUNDLE_DIR)/dmg/amenbo_$(VERSION)_$(if $(filter arm64,$(MAC_GUI_ARCH)),aarch64,x64).dmg
GUI_DMG_DIST := $(DIST_DIR)/amenbo-app-darwin-$(MAC_GUI_ARCH).dmg
# Unified installer: the .pkg drops the GUI in /Applications AND puts the
# bundled CLI on PATH (postinstall symlink). This is the primary mac installer;
# the dmg stays as a supplement for non-installer users.
GUI_PKG_DIST := $(DIST_DIR)/amenbo-darwin-$(MAC_GUI_ARCH).pkg
# A release ships both, so it names them both — MAC_GUI_ARCH selects one build,
# not what goes out.
GUI_PKG_ARM64 := $(DIST_DIR)/amenbo-darwin-arm64.pkg
GUI_PKG_AMD64 := $(DIST_DIR)/amenbo-darwin-amd64.pkg
# Tauri updater artifact for macOS (GUI self-update). Built ONLY when the minisign signing key is in
# the environment (release CI: secret TAURI_SIGNING_PRIVATE_KEY); ordinary dev/dist builds never
# require it. Unlike tauri's createUpdaterArtifacts (which tars the .app BEFORE the stable-identity
# re-sign), we tar the RE-SIGNED .app ourselves so the updater delivers the same fixed-leaf .app the
# installer does (notification authorization survives updates). The tar is AppleDouble-free
# (COPYFILE_DISABLE) — a stray ._ file breaks the update's signature seal — then minisign-signed.
MAC_UPDATER_DIST := $(DIST_DIR)/amenbo-darwin-$(MAC_GUI_ARCH).app.tar.gz
# Linux GUI bundles are built inside a container for an explicit arch, pinned via --platform.
# amd64 is the default (Linux desktop is overwhelmingly x86_64); override with
# LINUX_GUI_ARCH=arm64. deb uses amd64/arm64, rpm/AppImage use x86_64/aarch64, matching
# each tool's convention. The .deb/.rpm ship the GUI + CLI on PATH; AppImage stays GUI-only.
#   IMPORTANT: on Apple Silicon, amd64 is emulated via qemu and the Tauri CLI ABORTS
#   (SIGABRT) mid-bundle — so amd64 does NOT build reliably here. The amd64 release build
#   runs on a native x86_64 runner instead (.github/workflows/release.yml). Local
#   `make dist-gui-linux` is for the NATIVE arch (arm64 on this mac); it reuses that exact
#   recipe so the CI path and the local path are identical.
LINUX_GUI_IMAGE   := amenbo-linux-gui:latest
LINUX_GUI_ARCH    ?= amd64
LINUX_IMG_ARCH    := $(if $(filter arm64,$(LINUX_GUI_ARCH)),aarch64,x86_64)
GUI_APPIMAGE_DIST := $(DIST_DIR)/amenbo-app-linux-$(LINUX_IMG_ARCH).AppImage
GUI_DEB_DIST      := $(DIST_DIR)/amenbo-app-linux-$(LINUX_GUI_ARCH).deb
GUI_RPM_DIST      := $(DIST_DIR)/amenbo-app-linux-$(LINUX_IMG_ARCH).rpm
# The GUI e2e (verify-gui-linux) must run the HOST's arch: an emulated (qemu) amd64 build
# aborts in the Tauri CLI, and an emulated GUI is not what we want to watch anyway.
HOST_GUI_ARCH     := $(if $(filter arm64,$(shell uname -m)),arm64,amd64)
GUI_DEB_HOST      := $(DIST_DIR)/amenbo-app-linux-$(HOST_GUI_ARCH).deb
# The Linux clippy container (lint-linux) reuses Dockerfile.linux-gui but is built for
# the HOST arch, so it gets its own tag: the same tag under two platforms would have the
# dist image (amd64 by default) and this one overwrite each other on every build.
LINUX_LINT_IMAGE  := amenbo-linux-lint:$(HOST_GUI_ARCH)

# What shellcheck covers = every tracked shell script. Enumeration is left to git, so a new .sh is
# guarded automatically (nothing is forgotten). Shell embedded in workflows (`run:`) is not a file,
# so it does not appear here = shell-gate's actionlint sees that.
SHELL_SOURCES := $(shell git ls-files '*.sh' '.githooks/*')

.PHONY: help install install-dev gui gui-dev install-gui install-gui-dev dev-build hooks lock verify lint-linux verify-gui-linux verify-network-linux verify-network-mac test doc-gate shell-gate comment-gate go-gate scopes-gate cli-name-gate sweep-stale dist-gui dist-gui-mac dist-gui-linux verify-existing-store release codesign-cert devtool

help:
	@echo "make install      - [retired] the prod CLI ships in the unified installer; release with make release"
	@echo "make install-dev  - install the dev CLI to ~/.cargo/bin/amenbo-dev (app-data: work.amenbo.amenbo-dev)"
	@echo "make test         - full gate (core/cli scale,e2e + app crate clippy/test + GUI typecheck/build/test)"
	@echo "make verify ARGS=\"...\" - run the CLI in a throwaway isolated store (leaves prod/dev app-data untouched)"
	@echo "make lint-linux   - clippy the Linux branch (cfg(target_os=\"linux\")) in a container = the same 2 jobs as CI's rust/app-rust (make test does not see them; needs Docker)"
	@echo "make shell-gate   - shellcheck tracked shell (scripts/, guards/, .githooks/) and actionlint the run: in workflows (automatic at the start of make test; needs shellcheck 0.10+/actionlint)"
	@echo "make comment-gate - audit every comment in the tree, and the prose and config values of every tracked file that carries no code, against esorp.yaml = the same commands CI runs (automatic at the start of make test; skipped without esorp)"
	@echo "make go-gate      - gofmt/vet/test the optional devtool module = the same checks CI's go job runs (automatic at the start of make test; skipped without Go)"
	@echo "make shim-gate    - assert the GUI/CLI version-skew invariant holds (mac CLI symlinked into the .app, win CLI co-located in the per-user \$$INSTDIR) = the same guard CI runs (automatic at the start of make test)"
	@echo "make scopes-gate  - assert every dataset the change feed names is folded into a GUI scope (an unfolded table costs a full re-read) = the same guard CI runs (automatic at the start of make test)"
	@echo "make cli-name-gate - assert every command the CLI words takes its name from command_name() (a hardcoded name lies on the dev channel) = the same guard CI runs (automatic at the start of make test)"
	@echo "make sweep-stale  - if the cargo cache exceeds $(SWEEP_LIMIT_GB)GB, drop artifacts untouched for $(SWEEP_DAYS) days (automatic at the end of make test)"
	@echo "make dist-gui     - build the prod GUI (mac dmg) with build-time signing into dist/ (a supplement for non-installer users; not a wharfy bundle)"
	@echo "make dist-gui-mac - build the mac unified .pkg (GUI to /Applications, CLI to /usr/local/bin) into dist/ (the mac release bundle itself; Intel build via MAC_GUI_ARCH=amd64)"
	@echo "make dist-gui-linux - build the Linux GUI+CLI (.deb/.rpm bundling /usr/bin/amenbo) + AppImage in Docker into dist/ (needs Docker; partial build via BUNDLES=deb etc.)"
	@echo "make verify-gui-linux - exercise 'another process writes → the screen updates' on a real Linux GUI over Xvfb (needs Docker)"
	@echo "make verify-network-linux - stand up real NFS/SMB and exercise store_watch's network-FS detection (needs Docker; also runs every time in CI)"
	@echo "make verify-network-mac - the macOS version of the above (MNT_LOCAL detection). Mounts real SMB over loopback and exercises it (needs Docker)"
	@echo "make verify-existing-store - run the CLI bundled in the shipped .pkg against a clone of the prod store and check an existing store still opens and reads back (release runs this before publish)"
	@echo "make release      - [pre-tag gate only] just runs make test. Build and distribution are both public CI (release.yml on tag push -> prerelease, the promote workflow does the promotion) = there is no command here that distributes"
	@echo "make codesign-cert - one-time: create a stable self-signed certificate so install-dev/gui-dev stop re-prompting the keychain on every rebuild (macOS)"
	@echo "make lock         - re-resolve app/src-tauri/Cargo.lock (the GUI shell is outside the workspace, so a workspace bump leaves its lock behind; CI fails a PR whose lock is stale)"
	@echo "make hooks        - enable the git hooks (core.hooksPath=.githooks): pre-commit runs the tree guards over the staged diff, commit-msg holds the message to the same vocabulary"
	@echo "make devtool      - build the optional parallel-development helper to ~/.cargo/bin/devtool (needs Go; amenbo itself builds and tests without it)"
	@echo "make gui          - build the prod GUI (amenbo.app / work.amenbo.app)"
	@echo "make gui-dev      - build the dev GUI ($(GUI_DEV_NAME).app / $(GUI_DEV_ID))"
	@echo "make install-gui     - [retired] the prod GUI ships in the unified installer; release with make release"
	@echo "make install-gui-dev - build the dev GUI and put it in $(APPS_DIR)/$(GUI_DEV_NAME).app"
	@echo "                       AMB-T-ID=<id> builds that task's own throwaway instance (app-data work.amenbo.amenbo-dev-<id>) instead of the shared dev app; devtool task finish <id> deletes it"

## Re-resolve the lockfile of the GUI shell crate. That crate sits outside the workspace but reaches
## core through a path dependency, so a workspace bump changes what its lock resolves to — while the
## commit that made the bump never touches the file. Left alone the two drift apart silently, and the
## next person to build the app finds the churn in their working tree instead. `cargo metadata`
## re-resolves and rewrites the lock without compiling anything, so this needs none of the Tauri
## system libraries and takes seconds. CI runs the same command and fails if the file moves.
lock:
	cargo metadata --manifest-path app/src-tauri/Cargo.toml --format-version 1 >/dev/null
	@git --no-pager diff --stat -- app/src-tauri/Cargo.lock
	@echo "→ app/src-tauri/Cargo.lock re-resolved (commit it if it moved)"

## Tree guards: point the git hooks at .githooks.
hooks:
	git config core.hooksPath .githooks
	chmod +x .githooks/pre-commit .githooks/commit-msg guards/check-doc-refs.sh guards/check-test-prose.sh
	@echo "→ git hooks enabled (core.hooksPath=.githooks)"

## The prod CLI's make install is retired. The prod CLI ships in the unified installer and lands on
## PATH, so installing a separate cargo build invites a version-skew accident. Disabled to stop the
## mistake; it just points the way. The dev CLI is make install-dev; a release is tag push -> public
## CI -> the promote workflow.
install:
	@echo "✗ the prod CLI's 'make install' is retired (it ships in the unified installer)."
	@echo "  install prod : take the unified installer from the GitHub Release (ShiroDoromoto/amenbo) (mac .pkg / win NSIS / linux .deb, rpm)"
	@echo "  dev CLI      : make install-dev (~/.cargo/bin/amenbo-dev)"
	@echo "  release      : tag push → public CI (release.yml builds prerelease + attestation) → verify → the promote workflow. The local gate is make release"
	@exit 1

## Dev CLI: build into its own target dir with AMENBO_APP_NAME=amenbo-dev and place it as amenbo-dev.
install-dev: hooks dev-build
	cp target/dev/release/amenbo "$(CARGO_BIN)/amenbo-dev"
	@scripts/codesign-local.sh sign "$(CARGO_BIN)/amenbo-dev" work.amenbo.amenbo-dev
	@echo "→ amenbo-dev (dev; app-data: work.amenbo.amenbo-dev)"

## One-time: create a stable self-signed code-signing certificate and enable codesign for
## install-dev/gui-dev. This stops the keychain re-prompt on every rebuild (caused by ad-hoc
## signing = a shifting CDHash).
codesign-cert:
	@scripts/codesign-local.sh setup

dev-build:
	AMENBO_APP_NAME=amenbo-dev CARGO_TARGET_DIR=target/dev cargo build --release -p amenbo-cli

## Build the GUI artifact (mac dmg) into dist/ (a supplement for non-installer users). The mac
## release bundle itself is the unified .pkg (make dist-gui-mac); this dmg is not listed in
## wharfy.yaml. Signing: stable distribution signing is retired = the .app carrying tauri's
## build-time ad-hoc self-signature is packed straight into the dmg (arm64 execution is satisfied;
## the Gatekeeper first-run warning stays under self-signing regardless of the signature).
## This Mac produces the mac dmg only. The Linux bundle is make dist-gui-linux (Docker), the Windows
## bundle is produced separately on the ssh win host.
dist-gui:
	@mkdir -p $(DIST_DIR)
	cd app && npm run tauri build -- --target $(MAC_GUI_TRIPLE)
	cp "$(GUI_DMG_SRC)" "$(GUI_DMG_DIST)"
	@echo "→ $(GUI_DMG_DIST) (mac GUI dmg; .app is ad-hoc self-signed; distribution signing is retired)"
	@codesign --verify --deep --verbose=2 "$(GUI_DMG_DIST)" 2>&1 | head -1 || true
	@ls -1 "$(GUI_DMG_DIST)"

## Build the mac unified .pkg installer into dist/. Per-user: the GUI .app goes to
## ~/Applications and the bundled CLI (sidecar) is symlinked to ~/.local/bin/amenbo in postinstall
## = GUI+CLI from one installer, no elevation.
## Signing runs off ONE switch, MAC_SIGN_RELEASE (set by the release CI after
## import-signing-cert-mac.sh has loaded the Developer ID identities; unset for a local build, which
## then keeps tauri's ad-hoc signature and produces an unsigned container). With it on, the four
## steps below are ordered, not merely sequential:
##   1. codesign-release-mac.sh   — Developer ID Application, hardened runtime + timestamp
##   2. notarize-mac.sh app       — notarize and STAPLE the .app
##   3. build-pkg-mac.sh          — package the stapled .app, signed with Developer ID Installer
##   4. notarize-mac.sh pkg       — notarize and staple the finished installer
## The .app is stapled at step 2, BEFORE step 3 packages it and before the tar below — because the
## tar is the GUI self-update artifact, and a ticket stapled after it was tarred would never reach an
## updating user. Both the installed copy and the updated copy therefore validate offline.
## wharfy.yaml declares this .pkg as the mac BYO-bundle. The build runs on public CI (release.yml)
## on each OS's native runner.
## arch is MAC_GUI_ARCH (arm64 default / amd64 for the Intel build). The Intel build is a
## cross-build from Apple Silicon; the GUI and the bundled CLI sidecar both come from the --target
## slice.
dist-gui-mac:
	@mkdir -p $(DIST_DIR)
	cd app && npm run tauri build -- --target $(MAC_GUI_TRIPLE)
	scripts/codesign-release-mac.sh "$(MAC_GUI_APP)"
	scripts/notarize-mac.sh app "$(MAC_GUI_APP)"
	scripts/build-pkg-mac.sh "$(MAC_GUI_APP)" "$(GUI_PKG_DIST)" "$(VERSION)" "$(MAC_GUI_ARCH)"
	scripts/notarize-mac.sh pkg "$(GUI_PKG_DIST)"
	@ls -1 "$(GUI_PKG_DIST)"
	@# Updater artifact: tar the signed and STAPLED .app (AppleDouble-free) and minisign-sign it, only
	@# when the signing key is present (release CI). Skipped silently for keyless dev/dist builds.
	@if [ -n "$$TAURI_SIGNING_PRIVATE_KEY" ]; then \
	  COPYFILE_DISABLE=1 tar czf "$(MAC_UPDATER_DIST)" -C "$(MAC_BUNDLE_DIR)/macos" amenbo.app; \
	  ( cd app && npx tauri signer sign "$(CURDIR)/$(MAC_UPDATER_DIST)" ); \
	  echo "→ $(MAC_UPDATER_DIST) (+ .sig)"; \
	else \
	  echo "updater artifact skipped (TAURI_SIGNING_PRIVATE_KEY unset)"; \
	fi

## Run the shipped build (the CLI bundled in dist/'s mac .pkg) against a clone of the prod store and
## exercise whether **a store already out in the wild still opens and reads back**.
## It does not run automatically in the pre-tag gate — the release skill runs it against the
## prerelease bytes CI produced. It can also be run on its own before a release. The prod app-data is
## read-only (the clone is a throwaway AMENBO_HOME; KEEP=1 keeps it). Pass STORE_HOME= to point it at
## a different store.
verify-existing-store:
	@scripts/open-existing-store.sh "$(GUI_PKG_DIST)" $(if $(STORE_HOME),"$(STORE_HOME)",)

## Build the Linux GUI bundles (.deb + .rpm + AppImage) inside a Docker container and collect them
## into dist/. The .deb/.rpm bundle the GUI + CLI (`/usr/bin/amenbo`); AppImage is GUI-only.
## It never touches the mac host's toolchain / target/ / mac-native node_modules (it copies the
## source inside the container to build, and pulls back only the artifacts into dist/). Linux
## dependencies such as webkit2gtk-4.1 are isolated in scripts/docker/Dockerfile.linux-gui. Needs
## Docker. Signing follows Linux conventions (AppImage signing is optional).
dist-gui-linux:
	@command -v docker >/dev/null 2>&1 || { echo "✗ docker is required (the Linux GUI bundle is built with Docker)"; exit 1; }
	@mkdir -p $(DIST_DIR)
	docker build --platform linux/$(LINUX_GUI_ARCH) -f scripts/docker/Dockerfile.linux-gui -t $(LINUX_GUI_IMAGE) scripts/docker/
	@# Persist Tauri's tool cache (linuxdeploy/AppRun/appimagetool) across runs in a named
	@# volume so the AppImage step doesn't re-download from GitHub every build — repeated
	@# unauthenticated downloads get rate-limited (HTTP 429). XDG_CACHE_HOME points Tauri
	@# at the mounted volume. Per-arch volume (the tools are arch-specific).
	@# BUNDLES lets you build a subset (e.g. `make dist-gui-linux BUNDLES=deb`) for a fast
	@# partial verify; empty passes through as the container's default (deb,rpm,appimage).
	@# TAURI_SIGNING_* (release CI secret) flows through so the AppImage stage emits the signed
	@# updater artifact (.AppImage.sig); unset for a local build, where the AppImage stays
	@# unsigned (GUI-only, no updater artifact).
	docker run --rm --platform linux/$(LINUX_GUI_ARCH) \
	  -e VERSION="$(VERSION)" -e TARGET_ARCH="$(LINUX_GUI_ARCH)" -e XDG_CACHE_HOME=/cache \
	  -e BUNDLES="$(BUNDLES)" \
	  -e TAURI_SIGNING_PRIVATE_KEY -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD \
	  -v "amenbo-tauri-cache-$(LINUX_GUI_ARCH):/cache" \
	  -v "$(CURDIR):/src:ro" \
	  -v "$(CURDIR)/$(DIST_DIR):/out" \
	  $(LINUX_GUI_IMAGE) bash /src/scripts/docker/build-linux-gui.sh
	@echo "→ Linux GUI bundles built (arch=$(LINUX_GUI_ARCH); bundles=$(if $(BUNDLES),$(BUNDLES),deb,rpm,appimage)):"
	@# A partial build (BUNDLES=deb) leaves the others absent; list whatever landed rather
	@# than fail on the missing ones (the container already named the collected/missing set).
	@ls -1 $(DIST_DIR)/amenbo-app-linux-*.deb $(DIST_DIR)/amenbo-app-linux-*.rpm $(DIST_DIR)/amenbo-app-linux-*.AppImage 2>/dev/null || true

## The scenario that drives verify-gui-linux — the single source every driver reads. Its
## listed/present title is the card the check writes and OCRs back; override to point the
## Linux check at another scenario.
SCENARIO ?= verification/scenarios/task-appears-on-board.yaml

## Exercise "another process writes → the screen updates" on a real Linux GUI app. Put the .deb that
## dist-gui-linux built into a container with Xvfb and launch it, write via the CLI, and take
## before/after screenshots = confirm the spot where emit reaches WebKitGTK's webview (the last hop
## no other test touches).
## The judgment is mechanical (OCR): the title of the card the CLI wrote is absent from 1-before.png
## and present in 2-after.png. That title is not baked into the container — the host resolves it from
## $(SCENARIO) through the amenbo-scenario crate and passes it in (the container has no toolchain).
## Not on the always-on CI (it needs a full .deb build); it runs in the later stage of release.yml,
## which builds the bundle = catch the breaking trigger (a tauri/webview update) right before it
## ships.
verify-gui-linux: $(GUI_DEB_HOST)
	@command -v docker >/dev/null 2>&1 || { echo "✗ docker is required"; exit 1; }
	@command -v jq >/dev/null 2>&1 || { echo "✗ jq is required"; exit 1; }
	@mkdir -p $(DIST_DIR)/gui-e2e
	cp $(GUI_DEB_HOST) scripts/docker/
	docker build --platform linux/$(HOST_GUI_ARCH) -f scripts/docker/Dockerfile.linux-gui-e2e \
	  --build-arg DEB=$(notdir $(GUI_DEB_HOST)) -t amenbo-linux-gui-e2e:latest scripts/docker/
	rm -f scripts/docker/$(notdir $(GUI_DEB_HOST))
	@card="$$(cargo run -q --manifest-path verification/Cargo.toml -p amenbo-scenario --bin emit -- $(SCENARIO) \
	  | jq -r '([ .steps[] | select(.as != null) | {key: .as, value: .with.title} ] | from_entries) as $$labels \
	    | ([ .steps[] | select(.type == "assert" and .op == "listed" and (.with.present != false)) | .with.target ] | .[0]) as $$tgt \
	    | $$labels[$$tgt] // empty')"; \
	  [ -n "$$card" ] || { echo "✗ $(SCENARIO) has no listed/present title to drive the GUI check"; exit 1; }; \
	  echo "→ scenario card (from $(SCENARIO)): $$card"; \
	  docker run --rm --platform linux/$(HOST_GUI_ARCH) -e AMENBO_E2E_CARD="$$card" \
	    -v "$(CURDIR)/$(DIST_DIR)/gui-e2e:/out" amenbo-linux-gui-e2e:latest
	@echo "→ screenshots: $(DIST_DIR)/gui-e2e/{1-before,2-after,3-diff}.png"

$(GUI_DEB_HOST):
	$(MAKE) dist-gui-linux BUNDLES=deb LINUX_GUI_ARCH=$(HOST_GUI_ARCH)

## Stand up a real network FS (NFS/SMB) and exercise whether store_watch sees a store on it as
## "network" and wakes on polling. Get the detection wrong and the GUI misses other hosts' writes
## forever (inotify can be armed on NFS yet reports nothing). CI (ci.yml's app-rust) runs this every
## time, so it stays green. This is the entry point for running the same script locally — it runs in
## a privileged Linux container from mac (it needs mount(8) and the nfsd/cifs kernel modules).
verify-network-linux:
	@command -v docker >/dev/null 2>&1 || { echo "✗ docker is required"; exit 1; }
	docker build --platform linux/$(HOST_GUI_ARCH) -f scripts/docker/Dockerfile.linux-gui -t $(LINUX_GUI_IMAGE) scripts/docker/
	docker run --rm --privileged --platform linux/$(HOST_GUI_ARCH) \
	  -e CARGO_TARGET_DIR=/tmp/ctarget \
	  -v "$(CURDIR):/src" -w /src \
	  $(LINUX_GUI_IMAGE) bash scripts/verify-network-watch.sh

## The macOS version of the same verification. `is_network_dir` has a separate implementation per OS
## (mac holds no magic table and asks `statfs`'s MNT_LOCAL "is the backing store local?"), so a
## green Linux says nothing about this one. It stands up a samba container on 127.0.0.1:445 and
## loopback-mounts it with mount_smbfs = zero hands (using macOS file sharing would need a human in
## System Settings). It needs a real mount, so it is not on CI.
verify-network-mac:
	scripts/verify-network-watch-mac.sh

## The Windows counterparts of the two verifications above (the network-FS judgment, and "another
## process writes → the screen updates" on a real GUI) have no target here: Windows cannot be
## substituted with a container, so they drive a maintainer's own machine over ssh and run for nobody
## else. They live in .local/local.mk with the scripts they call, which are not tracked either.

## [pre-tag gate] The prod release is built by public CI — there is no "command that distributes"
## here at all. What this closes is only the gate before pushing a tag (version consistency and make
## test). On tag push, release.yml builds every OS and produces a prerelease + attestation, and after
## the real byte stream is verified the promote workflow (a manual Actions dispatch) promotes
## prerelease→latest and publishes/verifies. Build and distribution both stay in CI; there is no line
## here that calls wharfy release / publish (no GITHUB_TOKEN needed either).
## The migration rehearsal (running the shipped bytes against a prod clone) is done by the release
## skill against the bytes CI built, not a local build. The version is Cargo.toml [workspace.package]
## version (bump it first, then run).
release:
	@echo "→ pre-tag gate v$(VERSION) (build and distribution are both public CI; this is the gate only)"
	$(MAKE) test
	@echo "→ gate green. Pushing a tag makes public CI (release.yml) build every OS and produce a prerelease + attestation:"
	@echo "    git tag -a v$(VERSION) -m \"amenbo $(VERSION)\" && git push origin v$(VERSION)"
	@echo "  the prerelease does not become latest = users are unaffected. Once you verify the real artifact CI built"
	@echo "  (attestation + a migration rehearsal on a prod clone), the promote workflow (a manual Actions dispatch)"
	@echo "  promotes prerelease→latest and publishes/verifies. There is no command here that distributes."

## Full test: run cargo-nextest with the heavy gates (scale,e2e) included. Process isolation +
## parallelism make it fast, and each test's duration is listed. doctests are outside nextest, so
## `cargo test --doc` picks them up separately — the typed SQL layer's guarantee can only be proven by
## `compile_fail` doctests (proof that a wrong column name / type mismatch "fails").
## If nextest is not installed, it points the way (`cargo install cargo-nextest` or
## https://get.nexte.st).
## The app crate (the Tauri host, amenbo-app) is excluded from the root workspace, so the nextest/
## doctest above do not touch it = a core change that breaks only the GUI side is not caught by make
## test. The same gate as CI's app-rust + gui-web jobs (the app crate's clippy/test + GUI
## typecheck/build/test) runs here too, catching it locally before it slips into main. The GUI tests
## are only the host-independent lightweight ones (do not add heavy features).
## **But this gate sees only the OS it ran on**: code under `#[cfg(target_os = "linux")]` / `windows`
## is not even compiled on mac, so clippy violations and type errors there slip past a full green into
## main.
## To exercise the Linux branch run `make lint-linux` (Docker, below) = cross-compilation is not
## possible (Tauri's glib/gtk sys crate build.rs demands pkg-config's cross setup and fails), so it
## borrows a Linux box. **The Windows branch (`#[cfg(windows)]`) is not seen by the usual CI** —
## ci.yml's jobs are all ubuntu, and Windows is compiled only at a public-CI release (the windows job
## in release.yml).
test:
	@command -v cargo-nextest >/dev/null 2>&1 || { echo "cargo-nextest is required: cargo install cargo-nextest (or https://get.nexte.st)"; exit 1; }
	## The GUI gate below runs from app/node_modules (tsc, vite). A fresh clone has none — gitignored —
	## so without this the run walks minutes of Rust and dies at `tsc: command not found`. Fail here
	## with the one command that fixes it, the same way the toolchain checks above do.
	@[ -d app/node_modules ] || { echo "app/node_modules is missing: cd app && npm ci"; exit 1; }
	## Look at shell and comments first (a few seconds). No reason to wait 8 minutes of Rust for a
	## broken script, or for a comment CI is going to refuse.
	$(MAKE) --no-print-directory shell-gate
	$(MAKE) --no-print-directory comment-gate
	$(MAKE) --no-print-directory go-gate
	$(MAKE) --no-print-directory shim-gate
	$(MAKE) --no-print-directory scopes-gate
	$(MAKE) --no-print-directory cli-name-gate
	## Two runs, not one: the same split CI makes (ci.yml), so the heavy e2e suite never shares the
	## box with the scale seeds. The build is shared, so the second run only schedules tests.
	cargo nextest run --features scale,e2e -E 'not binary(cli_e2e)'
	cargo nextest run --features scale,e2e -E 'binary(cli_e2e)'
	cargo test --doc --features scale,e2e
	$(MAKE) --no-print-directory doc-gate
	## The app crate's build.rs (tauri_build) checks that tauri.conf.json's externalBin=binaries/amenbo
	## exists. The sidecar CLI is generated by tauri's beforeBuildCommand, but that does not run under a
	## direct cargo invocation, and binaries/ is gitignored, so a fresh worktree fails every time with
	## `resource path … doesn't exist`. Pre-generate it with the same script the GUI build uses.
	node app/scripts/prepare-cli-sidecar.mjs
	cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings -D clippy::disallowed_methods
	cargo test --manifest-path app/src-tauri/Cargo.toml
	cd app && npm run typecheck && npm run build && npm test
	## Sweep last. By the time we get here the build has touched core/cli, the app crate and the GUI, so
	## the live artifacts' atime is fresh. Sweeping before the build would drop assets not yet read (the
	## bench criterion data etc.) and cold them. Under the threshold it is a no-op that exits after one
	## `du`.
	@$(MAKE) --no-print-directory sweep-stale

## Run the Linux-target clippy from mac. The code under `#[cfg(target_os = "linux")]` that `make test`
## does not see — store_watch's inotify/statfs paths are the real thing — is only checked once it is
## **compiled** on Linux. Cross-compilation does not reach it (Tauri's glib/gtk sys crate build.rs
## demands pkg-config's cross setup), so it borrows the same container as the Linux GUI bundle (with
## webkit/gtk, Dockerfile.linux-gui) and runs the same 2 clippy jobs as CI's `rust` + `app-rust`
## inside it = if this is green those 2 jobs are green.
## **Not in `make test`**: the container build and a full Linux-side compile take minutes, and that
## tax does not fit a change that touched mac only. The guard against forgetting to run it is CI
## (`rust`/`app-rust` always run on every push) = this is **redundancy to fail before CI**, not a
## replacement for CI.
## It never touches the local target/ or mac-native node_modules (it copies into the container to
## build). Later runs are fast because named volumes (registry + two target dirs) carry over. Needs
## Docker.
lint-linux:
	@command -v docker >/dev/null 2>&1 || { echo "✗ docker is required (the Linux-branch clippy runs in a container)"; exit 1; }
	docker build --platform linux/$(HOST_GUI_ARCH) -f scripts/docker/Dockerfile.linux-gui -t $(LINUX_LINT_IMAGE) scripts/docker/
	docker run --rm --platform linux/$(HOST_GUI_ARCH) \
	  -v "amenbo-lint-registry-$(HOST_GUI_ARCH):/root/.cargo/registry" \
	  -v "amenbo-lint-target-$(HOST_GUI_ARCH):/build/target" \
	  -v "amenbo-lint-target-app-$(HOST_GUI_ARCH):/build/app/src-tauri/target" \
	  -v "$(CURDIR):/src:ro" \
	  $(LINUX_LINT_IMAGE) bash /src/scripts/docker/lint-linux.sh

## Mechanically guard that doc comments do not point at a **symbol that does not exist**. An intra-doc
## link to a removed function sends the reader to code that is not there, and prose discipline does
## not stop it. Running with `--document-private-items` is because a private crate's doc reader can
## see private items too — a link to a private item is correct (the crate allows
## `private_intra_doc_links`) and only a **broken link** fails. The app crate is a separate workspace,
## so run it separately (build.rs checks the sidecar exists, so pre-generate it).
doc-gate:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
	node app/scripts/prepare-cli-sidecar.mjs
	RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path app/src-tauri/Cargo.toml --no-deps --document-private-items

## Guard shell mechanically. scripts/ drives packaging and real-machine verification itself (mac pkg,
## Linux Docker, real-machine e2e); guards/ and .githooks/ are the side that stops a commit. An
## undefined variable, a quoting accident, a mixed-up `$@` all slip past `bash -n` (syntax only), and
## you notice on release day.
## Two layers:
##   (1) shellcheck … every tracked shell that exists as a file. `-x` follows `source=` directives =
##       a script's shared helpers are resolved and inspected too (the directive is SCRIPTDIR-relative,
##       so it works from any CWD).
##   (2) actionlint … shell **embedded** in workflows (`run:` in `.github/workflows/*.yml`).
##       actionlint feeds run: to shellcheck as shell = the same check as (1) reaches the embedded
##       side (and it also validates the workflow's own expressions and job references) = embedded
##       shell is guarded this way too.
## A Makefile recipe cannot be fed to shellcheck. So a recipe with substance is split out into
## scripts/ (verify-cli.sh has that shape) = once split out, (1) sees it. The judgment itself
## (including shellcheck's minimum-version check) lives in scripts/shell-gate.sh, which is itself a
## target of (1) = the gate guards the gate.
## Fix a finding, or drop it to a `# shellcheck disable=` with a reason (do not suppress silently).
## CI (ci.yml's shell job) calls this target too = the target set is not defined twice.
shell-gate:
	@scripts/shell-gate.sh $(SHELL_SOURCES)

## The devtool module's gate (gofmt/vet/test), the same three checks CI's `go` job runs — both call
## scripts/go-gate.sh, so the judgment is declared once and a verdict here is the verdict there.
## Skipped, quietly, where Go is not installed: devtool is optional and its toolchain is not a build
## dependency, so a clone without Go must still get a green `make test`. That is also why the skip is
## safe — CI has Go and always runs the job, so what is skipped here is still caught there.
go-gate:
	@if command -v go >/dev/null 2>&1; then \
	  scripts/go-gate.sh; \
	else \
	  echo "→ Go not installed — the devtool gate is skipped (it is optional; see devtool/README.md)"; \
	fi

## Audit every comment in the tree, and the prose and config values of every tracked file that
## carries no code, against the declarations in esorp.yaml — the same commands CI runs, so a verdict
## here is the verdict there.
## Both faces are judged whole, because both are clean: a violation anywhere is this change's to
## answer for, whichever line it sits on.
## The pre-commit hook runs them too, but only for someone who has run `make hooks`: this is what a
## fresh clone (an outside contributor) has, and without it their first sight of a violation is a red
## CI they cannot reproduce.
## Skipped, quietly, where esorp is not installed — it is not a build dependency.
comment-gate:
	@if command -v esorp >/dev/null 2>&1; then \
	  esorp check; \
	else \
	  echo "→ esorp not installed — comment gate skipped (see CONTRIBUTING.md)"; \
	fi
	@guards/check-prose.sh

## Guard the GUI/CLI version-skew invariant: the CLI on PATH must be the binary the GUI update
## replaces — a mac symlink into the .app, a win CLI co-located in the per-user $INSTDIR — never a
## frozen copy. A build is green either way, so this is the only thing that would notice a regression.
## Declared once and shared: `make test` and CI's tree-guards both run this file.
shim-gate:
	@guards/check-cli-shim.sh

## Guard the change feed's fold: every dataset core can name has to have a scope in the GUI's
## DATASET_SCOPES, or writes to that table silently fall back to re-reading everything. Both sides
## compile and pass their own tests while they disagree, so nothing else would notice.
## Declared once and shared: `make test` and CI's tree-guards both run this file.
scopes-gate:
	@guards/check-change-scopes.sh

## Guard the one name a build may tell someone to type: the CLI its channel installs. A hardcoded
## `amenbo` compiles, reads right, and even prints right in production — it only lies on the dev
## channel, where it names a command that is not there, and no test sees it (tests run production).
## Declared once and shared: `make test` and CI's tree-guards both run this file.
cli-name-gate:
	@guards/check-cli-name.sh

## Trim a bloated cargo cache by atime LRU. `target/` has no GC, and old artifacts with a different
## hash pile up forever (measured ~3.7GB/day). A periodic run would eat idle time, so sweep at the end
## of a build, and **only when it is fat**. Sweeping too much costs at most one rebuild (target is a
## regenerable cache).
sweep-stale:
	@SWEEP_LIMIT_GB=$(SWEEP_LIMIT_GB) SWEEP_DAYS=$(SWEEP_DAYS) scripts/sweep-stale-target.sh

## Run CLI real-machine verification in a throwaway isolated store. It never touches prod/dev
## app-data, so it avoids the accident of scattering test debris across the prod store listing. There
## are only 2 points of isolation:
##   (1) AMENBO_HOME=a throwaway dir … the only means to prevent app-data pollution (an isolated CWD
##       alone is not enough).
##   (2) a throwaway CWD with no `.amenbo` ancestor … prevents init from grabbing the prod pointer on
##       an in-repo run.
## It invokes the actual binary built from the current source (not cargo run = with (2)'s throwaway
## CWD it cannot find the manifest and always fails). Example: make verify ARGS="init --name Alice".
## Use KEEP=1 to inspect the contents on failure.
## Isolation and cleanup (two mktemp + rm -rf) live in the script = a target of shell-gate.
verify:
	cargo build -q -p amenbo-cli
	@scripts/verify-cli.sh "$(CURDIR)/target/debug/amenbo" $(ARGS)

## Prod GUI (local build). Stable distribution signing is retired = this does not use a distribution
## identity; it signs with the local stable self-signature (codesign-local) only to stop the dev
## box's rebuild re-prompt. Prod distribution is make release.
## This bundle points at the PRODUCTION app-data (that is what "prod" means here), but it carries no
## release stamp, so launching it cannot migrate that store forward. The stamp (`AMENBO_BUILD`) is
## set in release.yml only = never add it here.
gui:
	cd app && npm run tauri build
	@scripts/codesign-local.sh sign "$(GUI_APP)"
	@echo "→ amenbo.app (prod): app/src-tauri/target/release/bundle/macos/amenbo.app"

## Dev GUI: the dev identifier/productName and the dev AMENBO_APP_NAME. AMB-T-ID=<id> swaps all three
## for that task's throwaway instance (see the GUI_DEV_* block above). The second --config is
## merged over the file, so one recipe covers both shapes; with AMB-T-ID unset it merely restates what
## tauri.dev.conf.json already says.
gui-dev:
	cd app && AMENBO_APP_NAME=$(GUI_DEV_DATA) npm run tauri build -- --config src-tauri/tauri.dev.conf.json --config '{"productName":"$(GUI_DEV_NAME)","identifier":"$(GUI_DEV_ID)"}'
	@# Tauri emits an ad-hoc (linker-signed) .app whose CDHash changes every rebuild,
	@# re-prompting the keychain each cycle. Sign with the stable local identity so
	@# one "Always Allow" survives future rebuilds (matches install/install-dev).
	@scripts/codesign-local.sh sign "$(GUI_APP_DEV)"
	@# Say which frontend went in, and stop here if it is not the one just built. A bundle carrying
	@# an older frontend looks exactly like an implementation that does not work (scripts/verify-gui-front.sh).
	@scripts/verify-gui-front.sh "$(GUI_APP_DEV)"
	@echo "→ $(GUI_DEV_NAME).app (dev; $(GUI_DEV_ID))"

## Applying the prod GUI locally is retired (it is distributed via the unified installer). Replacing
## /Applications' prod amenbo.app locally clobbers the Developer ID signed, notarized build with a
## self-signed one — which changes the Designated Requirement and so drops the notification
## authorization — and causes version skew and keychain re-prompts. Apply prod only via tag push → public CI → the promote
## workflow → the unified installer.
install-gui:
	@echo "✗ the prod GUI's 'make install-gui' is retired (it is distributed via the unified installer)."
	@echo "  Replacing the prod amenbo.app locally clobbers the release-signed build."
	@echo "  install prod : take the unified installer from the GitHub Release (ShiroDoromoto/amenbo) (mac .pkg / win NSIS / linux .deb, rpm)"
	@echo "  dev GUI      : make install-gui-dev (amenbo (dev).app)"
	@echo "  release      : tag push → public CI (release.yml builds prerelease + attestation) → verify → the promote workflow. The local gate is make release"
	@exit 1

## Build the dev GUI and apply it to /Applications (quit it first if it is running, then replace).
## AMB-T-ID=<id> targets that task's own instance instead of the shared dev app.
install-gui-dev: gui-dev
	-osascript -e 'quit app "$(GUI_DEV_NAME)"' >/dev/null 2>&1
	rsync -a --delete "$(GUI_APP_DEV)/" "$(APPS_DIR)/$(GUI_DEV_NAME).app/"
	@# Check the copy that is actually clicked, not just the one that was built: the shared dev app is
	@# one bundle for the whole machine, so a parallel session can land its own build over this one.
	@scripts/verify-gui-front.sh "$(APPS_DIR)/$(GUI_DEV_NAME).app"
	@echo "→ updated $(APPS_DIR)/$(GUI_DEV_NAME).app (dev; app-data: work.amenbo.$(GUI_DEV_DATA))"

## The parallel-development helper (Go, one static binary): it stamps out and tears down a git
## worktree per task so several implementation sessions run without stepping on each other, and it
## measures what a diff does to the `amenbo agent --json` entry (devtool/README.md).
## It is **optional**. amenbo builds, tests and ships without it, so Go is not a dependency of the
## tree — it is asked for here and nowhere else, and whoever never runs this target never needs it.
devtool:
	@command -v go >/dev/null 2>&1 || { echo "✗ Go is required for devtool (it is optional; nothing else in this tree needs it)"; exit 1; }
	cd devtool && go build -o "$(CARGO_BIN)/devtool" .
	@echo "→ devtool (parallel development; $(CARGO_BIN)/devtool)"

## Local-only targets (each person's dev-environment tools) go in .local/local.mk, which is not
## tracked. If present it is included, if absent nothing happens. So it does not steal the default
## goal, the include happens after every target is defined = here.
-include .local/local.mk

