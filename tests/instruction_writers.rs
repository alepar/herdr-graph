//! R16: every raw file writer in non-test source is inventoried. A new one must be shown not to reach an
//! instruction-source path or to go through a plan kind committed by apply.
//!
//! Two inventories: the git-store API (`put_file` / `delete_file`) and raw `std::fs` / `File` writers. Writers
//! through other APIs (git2 checkout, shell commands) are still not detected.

use std::collections::BTreeSet;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs")
            && p.file_name().is_some_and(|n| n != "tests.rs")
        {
            out.push(p);
        }
    }
}

/// Non-test source files whose production lines (before the first `#[cfg(test)]`) satisfy `hit`.
fn files_with(hit: impl Fn(&str) -> bool) -> BTreeSet<String> {
    let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut files = Vec::new();
    walk(src, &mut files);
    let mut found = BTreeSet::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let calls = text
            .lines()
            .take_while(|l| !l.starts_with("#[cfg(test)]"))
            .any(&hit);
        if calls {
            let rel = f.strip_prefix(src.parent().unwrap()).unwrap();
            found.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    found
}

#[test]
fn std_fs_writers_are_inventoried() {
    const PATTERNS: [&str; 6] = [
        "fs::write(",
        "File::create(",
        "OpenOptions::new(",
        "fs::rename(",
        "fs::copy(",
        "fs::remove_file(",
    ];
    let found = files_with(|l| PATTERNS.iter().any(|p| l.contains(p)));
    let expected: BTreeSet<String> = [
        // Crash-atomic temp file + rename helper; callers pass state-dir paths (plan store, observe baseline,
        // .graph-local/ dirty and view-rev markers) or a committed blob for the worktree fast-forward.
        "src/fsutil.rs",
        // `init --with-examples` writes the example template files and commits them in the same call
        // (refuses to overwrite existing ones); instruction text is read from that commit, not the file.
        "src/bootstrap/examples.rs",
        // harness config dir (settings.json, hooks.json, rules/herdr-graph.rules, setup manifest); never the instance repo.
        "src/bootstrap/setup_claude.rs",
        // daemon.lock / daemon log under the instance's .graph-local/ state dir.
        "src/daemon/ensure.rs",
        "src/daemon/lock.rs",
        // removes the daemon socket under .graph-local/.
        "src/daemon/mod.rs",
        // isolation-violations log under the isolated test root (test-support).
        "src/herdr/isolation.rs",
        // expired stored plans under .graph-local/ (state dir).
        "src/plan/store.rs",
        // `init`: graph.toml, .gitignore and .gitkeep placeholders in the new repo (committed by init) and the
        // user config file naming the instance; no rules/ or AGENTS.md file.
        "src/store/init.rs",
        // thread namespace marker file in the plugin state dir.
        "src/threads/namespace.rs",
        // spool dir of session reports under .graph-local/ and its rejected/ quarantine.
        "src/transcripts/capture.rs",
        // removes stale git lock files (refs/heads/main.lock, index.lock) in .git.
        "src/writer/recovery.rs",
        // fast-forward of the working tree to the committed revision (committed blob bytes only; removes
        // deleted committed files, moves orphaned files into .graph-local/orphans/).
        "src/writer/worktree.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        found, expected,
        "a new std::fs writer must be shown not to write an InstructionSource path (rules/**, seat AGENTS.md, \
         template AGENTS.md) in the instance working tree, or to commit it through a plan kind (R16), then be \
         added here with its reason"
    );
}

#[test]
fn instruction_source_writers_are_inventoried() {
    let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    let mut files = Vec::new();
    walk(src, &mut files);
    let mut found = BTreeSet::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let mut prod = String::new();
        for line in text.lines() {
            if line.starts_with("#[cfg(test)]") {
                break;
            }
            prod.push_str(line);
            prod.push('\n');
        }
        let calls = prod.lines().any(|l| {
            (l.contains(".put_file(") || l.contains(".delete_file("))
                && !l.contains("fn put_file")
                && !l.contains("fn delete_file")
        });
        if calls {
            let rel = f.strip_prefix(src.parent().unwrap()).unwrap();
            found.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    let expected: BTreeSet<String> = [
        // InstructionsKind::mutate is the only writer of instruction paths (rules/**, seat AGENTS.md).
        "src/plan/instructions.rs",
        // template kinds write under templates/ only.
        "src/templates/kinds.rs",
        // threads reconnect writes mutations/threads-reconnect/ and the namespace marker.
        "src/threads/reconnect.rs",
        // request_complete writes the derived summaries/<tr>-<a>-<b>.md.
        "src/transcripts/mutations.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        found, expected,
        "a new raw file writer must be shown not to reach an InstructionSource path (rules/**, seat AGENTS.md) \
         or go through a plan kind committed by apply (R16), then be added here with its reason"
    );
}
