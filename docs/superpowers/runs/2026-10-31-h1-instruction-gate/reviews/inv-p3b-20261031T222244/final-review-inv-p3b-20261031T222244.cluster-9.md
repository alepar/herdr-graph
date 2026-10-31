# Final review: cluster 9 (src-root), epic hg-45e

Source: base c77d9ad166dd2ada5b5a5768b499d12da4c36987, head bce6589afead4c031ac9ecfa3a094c9a5c4be161 (both verified with `git rev-parse --verify`). Caller checkout observed at HEAD bce6589, working tree clean; not modified by this review. Review source bound via `dispatch-text.mjs bind-review-source` (exit 0).

## Scope

- `src/config.rs` (slice `slices/000048.diff`, +6 lines): adds `Env::claude_config_dir` (`CLAUDE_CONFIG_DIR`) and `Env::codex_home` (`CODEX_HOME`), populated in `Env::from_process` through the existing `var()` helper.

## Assessment against the spec

- Spec §3.4 requires doctor's `harness_gate` checks to "read the same config paths setup uses (`config_dir`/`codex_dir`)". The only consumer of the new fields is `src/daemon/doctor.rs:465-466`, which passes them into `setup_claude::config_dir` / `setup_claude::codex_dir` — the same resolvers `src/cli/setup.rs:39,49` uses. Requirement met.
- Empty-value semantics agree with setup: `config::var` filters empty strings, and `cli/setup.rs::env_path` also filters empty `OsString`s, so `CLAUDE_CONFIG_DIR=` falls back to `~/.claude` in both doctor and setup. No divergence between what setup installs and what doctor checks.
- `Env` derives `Default`; no struct literal outside `from_process` names every field, so the additions do not break other constructors (the build at head compiles these call sites).
- The fields are plain path data; `tripwire` is applied downstream in `config_dir`/`codex_dir`, so the isolation guard still covers the resolved paths.
- Pre-existing, outside this cluster: `src/daemon/mod.rs:80` reads `CLAUDE_CONFIG_DIR` with `var_os` and no empty filter, so the daemon treats an empty value differently from setup/doctor. Not introduced by this branch; noted only.

## Defects

- None found in this cluster.

## Verdict
ready

## Must fix before landing
- none

## Untested scope
- No test drives `Env::from_process` with `CLAUDE_CONFIG_DIR`/`CODEX_HOME` set through to `doctor()`'s `harness_gate` checks; the unit tests in `src/daemon/doctor.rs` call `harness_gate_checks` with explicit paths, so the env-to-path wiring at `doctor.rs:465-466` is covered only by inspection.

## Deferred OK
- `src/daemon/mod.rs:80` reads `CLAUDE_CONFIG_DIR` via `var_os` without the empty-value filter used by `Env`/setup; pre-existing, could migrate to `Env::claude_config_dir` as a follow-up for consistency.
