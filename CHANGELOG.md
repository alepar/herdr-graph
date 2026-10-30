# Changelog

## v0.2.0 — 2026-10-30

### Added

- **Codex setup.** `herdr-graph setup codex [--uninstall]` installs an owned SessionStart hook in `${CODEX_HOME:-~/.codex}/hooks.json`, plus the `seat` and `graph` skills. `--uninstall` removes exactly that. Codex runs the hook only after you trust it in an interactive session (the startup review or `/hooks`); setup never writes trust.
- **Linux.** The plugin manifest lists `linux`. Tests, fixtures, the long-path socket fallback and the release matrix driver all run on Linux x86_64.

### Changed

- **Seat role comes back after compaction.** Claude and Codex fire SessionStart on startup, resume, clear and compaction, and graph's hook now tells a graph pane's session why it must run the seat skill: a summary of an earlier `/seat` run is not a substitute. The old two-word `run /seat` was mostly ignored, above all after compaction (1 of 4 runs). The new text led to `/seat` in 12 of 12 Claude runs and in a live Codex run.
- **herdr-threads 2.6.4.** Pinned to official v2.6.4 (`cdc8423f`), wire protocol 9, and also verified against a 2.7.0 daemon. Threads 2.4.1–2.6.0 upgrade the threads store one way, to node schema 10.

### Fixed

- **No duplicate after a crash.** Recovery after a daemon crash no longer leaves a duplicate workspace or tab. Herdr can apply a create still buffered from the crashed daemon after recovery has started; graph now closes such a copy when it is still unused. If the copy is in use, graph reports it instead of closing it. A pane from a lost split is not covered.

### Packages

`herdr-graph-linux-x86_64.tar.gz` contains the Linux x86_64 release binary (vendored libgit2), plugin manifest, templates, skills, documentation and source, with the exact threads v2.6.4 source bundled at `third_party/herdr-threads`. `herdr-graph-source.tar.gz` contains the source with the same bundled threads source. `SHA256SUMS` covers both archives. No macOS binary is attached to this release, and the macOS sweep was not rerun at threads 2.6.4.

## v0.1.0 — 2026-10-07

First published release. Version matches the existing Cargo and plugin metadata; no crates.io publication (`publish = false`). Tag targets the reviewed and verified main commit `6c9db8f5d81cf1771d0afd3ea8e1d37a9c8027fa`.

### Changes

- Reusable seat and team templates with independent seats, explicit seat reuse, layered live instructions and runtime precedence: seat instance > team member > seat template > team template > graph defaults > built-ins. SystemDuty replaces profession-role machinery while retaining legacy serialized/CLI aliases and summarizer behavior.
- Human-browsable transcript indexes grouped by registration-time team/seat names and full stable IDs. Immutable registration attribution survives organizational changes; native transcripts are never copied or rewritten.
- Purpose-specific mutation records under `mutations/graph-changes`, `mutations/undoable-actions` and `mutations/transcript-processing`, retaining legacy readers and undo paths.
- Reviewed template instruction edits preserve stable member specialization through renames, name swaps and undo; ambiguous instruction slugs are rejected.
- Updated design, graph-state, authoring and CLI documentation, with preserved historical review artifacts.
- herdr-threads v0.2.9 compatibility, pinned to official release commit `223b61a88625d7f442d22d9b8728dc4b2282b15f` (wire protocol 6).

### Packages and installation

`herdr-graph-macos-aarch64.tar.gz` contains the macOS arm64 release binary, plugin manifest, build script, templates, skills, documentation and bundled source. macOS arm64 is the built and exercised architecture; no Intel macOS or Linux binary is supplied. Requires Herdr >= 0.9.1.

`herdr-graph-source.tar.gz` contains source with the exact threads v0.2.9 source bundled at `third_party/herdr-threads`; neither asset relies on the repository's absolute local dependency symlink. `SHA256SUMS` covers both archives. `RELEASE-PROVENANCE.json` inside each package identifies both commits. Dependencies from crates.io still require Cargo's cache or network for a source build.

Verify checksums, extract the chosen archive, then follow `README.md`. The prebuilt binary is `herdr-graph/bin/herdr-graph`. It is built with vendored libgit2 to avoid a Homebrew runtime dependency. Source builds use `scripts/build.sh`; to reproduce the portable binary, run `LIBGIT2_NO_VENDOR=0 cargo build --locked --release --bin herdr-graph --features libgit2-sys/vendored`, then copy `target/release/herdr-graph` to `bin/herdr-graph`. Link/setup commands are deliberate local actions and can activate the plugin; release publication does not install or restart anything. GitHub's automatic source archives retain the original absolute symlink: use the attached bundled source archive for portable builds.

### Compatibility and migration

Back up both the instance Git repository and SQLite journal before upgrading. There is no eager relocation: legacy transcript/request/action/operation paths remain readable and existing records update in place. Old compensation paths remain valid. New transcript folders retain registration-time names; legacy attribution stays unknown. Native transcript/history bytes remain untouched.

Older binaries cannot discover newly grouped records; downgrade discovery is unsupported. Saved plans can require re-planning. Direct template instruction writes must use `template edit`. Template create/edit documents reject colliding instruction slugs (including inverse documents); existing stored templates remain readable. Previously lost instruction text is not reconstructed. Existing native resume/replacement limits remain.

Targeted duplicate/corrupt IDs error; full enumeration detects history-wide corruption. Only documented layout depths are supported. No large-history latency benchmark is claimed. The graph's SQLite journal remains authoritative for live/rejected/failed/cancelled operation lifecycle.

### Verification and limits

Implementor's fresh merged-main default suite: 670 passed, 0 failed, one intentionally ignored release-build packaging test. Earlier default/no-default/test-support/private-server matrices and task reviews are recorded in [the integration handoff](https://github.com/alepar/herdr-graph/blob/6c9db8f5d81cf1771d0afd3ea8e1d37a9c8027fa/docs/releases/2026-10-07-agreed-fixes-handoff.md). The final member-rename correction has six red/green regressions and an approved scoped re-review.

Two opt-in isolated real-threads daemon tests actually ran and passed. They use synthetic public-hook registration and manual cooperative CLI invitation acceptance/read/ACK; ACK proves receipt/dispatch, not completed processing. Native model launch/bootstrap/root-agent/summarizer behavior remains unexercised. No live service or plugin was restarted.

Release-owner checks on a clean exported bundle: `cargo test --locked --offline` reports **670 passed, 0 failed, 1 ignored**; the normally ignored release-build packaging regression was separately run and **passed (1/0/0)**. The vendored optimized build succeeded, and its isolated release integration sweep reports **7 passed, 0 failed**. Formatting and source-byte comparisons passed. Binary reports 0.1.0, is Mach-O arm64, passes ad-hoc signature verification and links only macOS system libraries. Both downloaded draft archives are compared byte-for-byte and checked against SHA256SUMS before publication.

### Publication policy for this first release

Keep Cargo/plugin versions aligned. Publish an annotated `v<version>` tag on the exact verified commit after integration ownership transfer. Build locked from a clean export with pinned dependency source; supply platform and source archives plus checksums. Create a GitHub draft, upload and verify downloaded assets before publication. The release notes serve as this release's changelog. Do not claim platforms or integration cells that were not exercised.

