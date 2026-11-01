//! R16: every raw file writer in non-test source is inventoried. A new one must be shown not to reach an
//! instruction-source path or to go through a plan kind committed by apply.

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
