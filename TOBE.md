# Thread to Tab: Rust migration and binary distribution

## Goal

`toyamarinyon.thread-to-tab` should be installable and usable with one Herdr
command:

```sh
herdr plugin install toyamarinyon/herdr-thread-to-tab
```

The installed plugin must not require Python, Rust, Node.js, or a package
manager on the user's machine. Codex support may continue to require the
`codex` executable because its local app-server is the source of Codex thread
metadata.

The Rust version should preserve the behavior already validated by the Python
prototype:

- Listen for Herdr `pane.created` and `pane.updated` events.
- Synchronize eligible panes once at startup.
- Support Claude Code and Codex.
- Use Claude Code's stripped terminal title.
- For Codex, prefer thread `name`, then `preview`, then a safe terminal-title
  fallback.
- Ignore UUIDs, generic titles, and Codex project-name-only titles.
- Rename only single-pane tabs.
- Replace numbered default labels and labels previously written by this plugin.
- Preserve manually assigned labels.
- Truncate labels to 30 Unicode characters, including the trailing `...`.
- Continue recognizing a plugin-owned label after a Herdr restart changes the
  terminal ID.

This document describes the desired first public release. It is not a roadmap
for every possible future plugin feature.

## Non-goals

The migration should not introduce:

- A general plugin framework or reusable Herdr SDK.
- Async execution solely because Rust supports it.
- A configuration system for constants that have no demonstrated need to vary.
- Multi-pane title ownership. Multi-pane tabs remain untouched.
- Support for agents other than Claude Code and Codex.
- A long-running service separate from Herdr's startup process.
- Automatic Codex thread naming or changes to the user's Codex configuration.
- Telemetry, networking at runtime, or an auto-updater.
- Backward compatibility for undocumented Python APIs or test helpers.

If one small function is used in only one place, keep it local and direct.
Extract modules and abstractions only where they clarify a real boundary or
make behavior independently testable.

## Target repository shape

The exact filenames may change slightly during implementation, but the final
repository should remain small and unsurprising:

```text
.
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── release.yml
├── scripts/
│   └── install-binary.sh
├── src/
│   ├── main.rs
│   ├── title.rs
│   ├── state.rs
│   ├── herdr.rs
│   └── codex.rs
├── Cargo.lock
├── Cargo.toml
├── LICENSE
├── README.md
├── TOBE.md
└── herdr-plugin.toml
```

Prefer fewer modules if the implementation is short. The useful boundaries
are title selection, state ownership, Herdr communication, and Codex
communication; they do not need traits or multiple implementations.

After the Rust version reaches parity, remove `bin/sync_tab_title.py` and the
Python tests. Do not retain two production implementations.

## Runtime design

### Process lifecycle

Herdr starts one plugin process from the manifest's `[[startup]]` entry. The
binary:

1. Validates the required Herdr environment variables.
2. Reads all existing panes and attempts one synchronization pass.
3. Opens the Unix socket from `HERDR_SOCKET_PATH`.
4. Subscribes to `pane.created` and `pane.updated`.
5. Processes matching events sequentially until the socket closes.

A blocking event loop is sufficient. Work is local and low-volume, and
sequential processing avoids locks and ordering surprises. Tokio or another
async runtime should only be introduced if a measured problem cannot be solved
cleanly with the standard library.

On socket closure or a fatal startup error, exit non-zero with a concise message
on stderr so `herdr plugin log list` remains useful. A malformed individual
event or a transient metadata lookup failure should be logged and skipped
without killing the listener.

### Herdr access

Keep the current pragmatic split:

- Use the injected `HERDR_BIN_PATH` for `pane list`, `pane get`, `tab get`, and
  `tab rename`.
- Use the Unix socket directly only for the event subscription.

This avoids implementing a broader socket client merely to replace stable CLI
commands. Parse only the response fields the plugin needs, using small
`serde` structs or carefully scoped `serde_json::Value` access. Do not model
the entire Herdr API.

All subprocesses need bounded execution time and captured stderr. Error
messages should identify the failed operation without printing pane contents or
the user's prompt.

### Title selection

Title cleanup must happen before filtering; truncation happens only after a
candidate is accepted. This preserves full UUID and project-title detection.

Normalization rules:

1. Collapse whitespace and trim the ends.
2. Reject empty strings and control characters.
3. Apply agent-specific filtering.
4. Truncate to 30 Unicode scalar values, with `...` occupying the final three.

Never slice a Rust string by byte index. Use `chars()` so Japanese text and
other multibyte UTF-8 input cannot panic or become invalid. Matching the current
Python behavior by Unicode scalar value is sufficient; grapheme-cluster-aware
truncation is unnecessary for this release.

Claude Code uses `terminal_title_stripped`.

Codex uses `agent_session.value` as the thread ID and queries:

```text
codex app-server
initialize
initialized
thread/read
```

The selection order is `name`, `preview`, then the captured terminal title.
The fallback must continue rejecting a UUID and the full or terminal-truncated
project directory name.

Do not add a persistent Codex app-server connection initially. It complicates
request correlation, restart handling, and child-process ownership. Start with
behavioral parity. If profiling shows excessive spawning from frequent pane
events, add the smallest effective optimization, such as a short in-memory
cache keyed by thread ID. Do not optimize based only on possibility.

The Codex child must have a timeout and must always be terminated and reaped.
Missing Codex, malformed JSONL, timeout, or an app-server error is a normal
fallback condition, not a listener crash.

### Manual-label protection and state

Keep state under:

```text
$HERDR_PLUGIN_STATE_DIR/titles.json
```

The first Rust release should read the existing Python JSON shape:

```json
{
  "terminal-id": "label previously written by the plugin"
}
```

That allows an in-place upgrade without unexpectedly treating the existing
plugin label as manual. A label is replaceable when:

- it consists only of decimal digits; or
- it equals the value stored for the current terminal ID; or
- it equals any label value previously stored by the plugin.

The last rule handles terminal IDs changing across Herdr restarts. It is
intentionally narrow: merely resembling an inferred title is not enough to
override a manual label.

Use an exclusive file lock while reading, deciding, renaming, and writing.
Write valid UTF-8 JSON and preserve compatibility with the existing file.
Avoid introducing a database, migrations framework, or versioned state schema
until the stored data actually needs a new shape.

State can accumulate old terminal IDs. The expected volume is tiny, so garbage
collection is YAGNI. If this becomes observable, add a simple bound later based
on evidence.

## Dependencies

Keep the dependency set deliberately small:

- `serde` and `serde_json` for protocol and state JSON.
- One small cross-platform file-locking crate if the standard library cannot
  provide the required lock on supported targets.

Prefer `std::process`, `std::os::unix::net::UnixStream`, and standard I/O and
time facilities. Avoid a general HTTP client because the runtime performs no
network requests; release asset download belongs in the installation script.

Commit `Cargo.lock` because this repository produces an application binary.
Pin a stable Rust edition and define a conservative minimum supported Rust
version only if CI verifies it.

## Plugin manifest and installation

The production startup command should execute the installed binary, for
example:

```toml
[[startup]]
command = ["bin/thread-to-tab", "--listen"]
```

The manifest should also declare the Herdr-supported build/install command that
runs `scripts/install-binary.sh`. Before implementing this field, verify its
exact syntax against the Herdr version targeted by `min_herdr_version`; do not
guess from an older preview manifest.

`scripts/install-binary.sh` should:

1. Detect OS and architecture with `uname`.
2. Map only supported combinations to an exact release asset name.
3. Read the plugin version from `herdr-plugin.toml` or receive it explicitly;
   the manifest version and Git tag must remain identical.
4. Download the matching asset and checksum from the GitHub Release for that
   exact version.
5. Verify SHA-256 before installation.
6. Place the executable at `bin/thread-to-tab` and mark it executable.
7. Fail clearly for unsupported targets or a missing release asset.

The script must use POSIX `sh`, quote all paths, enable fail-fast behavior, and
avoid modifying directories outside the managed plugin checkout. Check which
download and checksum tools are available on supported macOS and Linux systems,
with small explicit fallbacks where necessary. Do not build a general-purpose
installer library.

Do not silently fall back to `cargo build` in the user installation path. That
would make success depend on an undeclared Rust toolchain and obscure release
packaging failures. Local contributors can build with Cargo directly.

Herdr installation should remain:

```sh
herdr plugin install toyamarinyon/herdr-thread-to-tab
```

Local development remains:

```sh
cargo build
herdr plugin link .
```

The linked manifest must point to a predictable local binary. If production
installation and local development require different paths, solve that with a
small launcher script or a documented build step, not duplicated manifests
unless Herdr's manifest format requires it.

## Release artifacts

Build at least:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`

Use consistent names, for example:

```text
thread-to-tab-v0.1.0-aarch64-apple-darwin.tar.gz
thread-to-tab-v0.1.0-x86_64-apple-darwin.tar.gz
thread-to-tab-v0.1.0-aarch64-unknown-linux-gnu.tar.gz
thread-to-tab-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
SHA256SUMS
```

If GNU Linux artifacts prove too restrictive across supported distributions,
evaluate musl targets with actual installation tests. Do not add both GNU and
musl matrices preemptively.

Release archives should contain only the executable and essential license or
notice files. Strip release binaries where supported. Reproducible builds are
desirable, but a full reproducible-build system is outside the initial scope.

## CI and release workflow

### Pull-request and branch CI

Run on macOS and Linux:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Add a manifest/install-script validation test that does not download a real
release. The target-mapping and asset-name logic should be testable through
arguments or environment overrides.

Avoid large matrices that test equivalent behavior. Native macOS and Linux
coverage plus cross-compilation checks for release targets is enough.

### Tagged release

On a tag such as `v0.1.0`, the release workflow should:

1. Verify that the tag, `Cargo.toml` package version, and
   `herdr-plugin.toml` version agree.
2. Run formatting, linting, and tests.
3. Build the supported target binaries.
4. Package and checksum them.
5. Create a GitHub Release and upload all assets.
6. Perform a smoke installation against the release assets where practical.

The release must be published before the default branch advertises a manifest
version whose installer expects those assets. Use a release PR or otherwise
sequence version bumps and tag publication so a normal `plugin install` never
points at files that do not exist.

Keep GitHub Actions pinned to release tags initially. Pinning every action by
commit SHA can be considered when the repository adopts a broader supply-chain
policy; it should not block the functional migration.

## Test plan

Port every current Python behavior test to Rust. Unit tests should cover:

- whitespace cleanup and control-character rejection;
- 30-character truncation, including Japanese/multibyte input;
- generic title rejection;
- Codex UUID and project-title rejection;
- Codex `name` → `preview` → terminal fallback order;
- unsupported agents;
- numeric default label replacement;
- manual label preservation;
- plugin-owned label replacement;
- replacement after terminal ID changes;
- multi-pane refusal;
- nested Herdr event envelope parsing.

Use small fake clients around the synchronization decision. Prefer ordinary
structs and functions over a broad mock framework. A narrow trait may be used
only if it materially improves isolation of subprocess/socket boundaries.

Add integration tests for:

- Herdr CLI JSON parsing using fixed fixtures.
- Codex JSONL initialization and `thread/read` against a tiny fake child
  process or fixture-driven transport.
- State compatibility with a Python-generated `titles.json`.
- Installer target mapping and checksum rejection.

Finally, test manually in a linked real Herdr session:

1. Claude Code inferred title appears.
2. Codex named thread appears.
3. Codex unnamed thread uses preview.
4. A title longer than 30 characters ends in `...`.
5. A manual tab rename remains unchanged.
6. A multi-pane tab remains unchanged.
7. Restarting Herdr preserves ownership and permits the next automatic update.
8. Missing or failing `codex` does not stop Claude synchronization.

Do not make live Herdr tests mandatory in CI; they depend on session state and
installed agents.

## Migration sequence

Implement in small, reviewable checkpoints:

1. Create the Rust crate and port pure title-selection tests.
2. Port state ownership and compatibility tests.
3. Implement Herdr CLI calls and event subscription.
4. Implement the bounded Codex app-server client.
5. Link the Rust binary locally and verify parity in the real Herdr session.
6. Change the manifest startup command from Python to the Rust binary.
7. Remove Python production code and tests.
8. Update README requirements, installation, development, and troubleshooting.
9. Add installer target mapping and checksum verification.
10. Add CI and release workflows.
11. Publish a prerelease, install it into a clean temporary environment, and
    verify no Python or Rust toolchain is used.
12. Publish the stable release only after the one-command installation succeeds.

Each checkpoint should leave tests passing. Do not combine release automation
with an unverified behavioral rewrite if separating them makes failures easier
to diagnose.

## README changes required

The final README should lead with the user outcome and one-command install. It
should state clearly:

- Python and Rust are not runtime requirements.
- Herdr and, for Codex metadata, Codex CLI are the only required executables.
- Claude Code works from its default inferred terminal title.
- Codex uses `name || preview` through the local app-server API and does not
  require `[tui].terminal_title` configuration.
- Only single-pane tabs are synchronized.
- Manual labels are preserved.
- Labels are capped at 30 characters.
- A Herdr server restart may be required to start a newly installed startup
  listener, according to the behavior of the minimum supported Herdr version.
- How to inspect `herdr plugin list` and `herdr plugin log list`.
- How to uninstall with `herdr plugin uninstall`.

Keep implementation detail out of the opening section. Put contributor build
instructions after normal installation and behavior documentation.

## Acceptance criteria

The migration is complete when:

- No production path invokes Python.
- `cargo fmt`, `cargo clippy`, and all tests pass.
- The Rust listener matches the established behavior in a real Herdr session.
- Existing `titles.json` state is accepted without manual migration.
- Four supported release assets and verified checksums are published.
- A clean macOS installation succeeds with only Herdr, Git, standard system
  utilities, and Codex when testing Codex behavior.
- A clean Linux installation succeeds under the documented baseline.
- `herdr plugin install toyamarinyon/herdr-thread-to-tab` installs the correct
  binary without invoking Cargo.
- Manual labels and multi-pane tabs are never overwritten in the acceptance
  scenarios.
- README instructions describe the released behavior rather than the removed
  Python prototype.

## Implementation judgment

Favor correctness at the boundaries and simplicity inside them:

- Validate untrusted JSON and subprocess output.
- Put timeouts around child processes.
- Check release checksums.
- Preserve manual user choices.
- Keep the synchronous core straightforward.
- Measure before adding caches, async tasks, retries, or concurrency.
- Prefer deleting the Python implementation after parity over maintaining two
  versions.
- Treat speculative agent support, configuration knobs, state cleanup, and
  framework extraction as future work only when a concrete use case appears.

The best result is not the most extensible codebase. It is a small plugin whose
behavior can be understood from a few modules, whose installation has no hidden
language dependency, and whose failure modes are visible and safe.
