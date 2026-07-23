# Thread to Tab for Herdr

Keep Claude Code and Codex thread titles visible as Herdr tab labels. Install it
with one command:

```sh
herdr plugin install toyamarinyon/herdr-thread-to-tab
```

The plugin downloads a verified native binary during installation. Python,
Rust, Node.js, Cargo, and a package manager are not runtime requirements.

## Requirements

- Runtime: Herdr 0.7.0 or newer on macOS or Linux
- Runtime for Codex metadata only: Codex CLI
- Installation only: Git and standard system download/archive/checksum
  utilities

After installation, Herdr and—only for Codex metadata—the Codex CLI are the
only required executables.

Release binaries target macOS 11 or newer and GNU/Linux with glibc 2.17 or
newer on the four documented x86_64/aarch64 combinations.

Claude Code works from its default inferred terminal title. Codex reads the
thread `name`, then `preview`, through the local `codex app-server`; it does not
require a `[tui].terminal_title` setting. If Codex metadata is unavailable, a
safe captured terminal title may be used.

Herdr starts startup listeners while restoring a session. After installing,
restart the Herdr server once if the listener has not started. This restarts
pane processes, so do it at a convenient time.

## Behavior

- Only tabs containing exactly one pane are synchronized.
- Numeric default labels and labels previously written by this plugin can be
  replaced.
- Manually assigned labels are preserved.
- Generic agent titles, UUIDs, and Codex project-directory-only titles are
  ignored.
- Labels are capped at 30 Unicode characters, including the trailing `...`.
- Existing panes are synchronized once when the listener starts, then
  `pane.created` and `pane.updated` events are processed.
- State from the earlier Python version is accepted without migration.

No telemetry or runtime network requests are made. Release downloads occur
only during installation.

## Troubleshooting and removal

Inspect installation state and recent listener errors:

```sh
herdr plugin list --plugin toyamarinyon.thread-to-tab
herdr plugin log list --plugin toyamarinyon.thread-to-tab
```

Remove the managed checkout and registration:

```sh
herdr plugin uninstall toyamarinyon.thread-to-tab
```

If only Codex titles fail, confirm `codex` is on `PATH`. A missing, failing, or
slow Codex app-server does not stop Claude Code synchronization.

## Local development

Build the predictable binary path used by the linked manifest, then link the
checkout:

```sh
cargo build
mkdir -p bin
cp target/debug/thread-to-tab bin/thread-to-tab
herdr plugin link .
```

`plugin link` intentionally does not run manifest build commands. Rebuild and
copy the binary after code changes. Restart Herdr when you need it to launch a
new startup listener.

Run the local checks:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
sh tests/installer.sh
sh tests/release-assets.sh
sh tests/local-release-smoke.sh
```

Unlink without deleting the checkout:

```sh
herdr plugin unlink toyamarinyon.thread-to-tab
```

## Release process

The `v0.1.0` tag, `Cargo.toml` version, and `herdr-plugin.toml` version must
match. Push the release commit and tag from a release branch before merging it
into the default branch. The tagged workflow verifies the versions, builds and
validates the four supported targets, publishes a GitHub prerelease with
`SHA256SUMS`, and smoke-tests installation.

Test that prerelease on clean macOS and Linux hosts, including the manual Herdr
scenarios in `TOBE.md`. Promote the same GitHub Release to stable only after
those checks succeed, then merge the release commit. This keeps the default
branch from advertising a manifest version whose assets do not exist.
