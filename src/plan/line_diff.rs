//! Unified line diff for plan effect details (spec §2.4): LCS over lines, 3 lines of
//! context. No crate: builds run `--locked --offline`.

pub const CONTEXT: usize = 3;

/// Above this many table cells the LCS table is skipped (bounds memory on pathological input).
const MAX_CELLS: usize = 4_000_000;

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Equal(usize),
    Delete(usize),
    Insert(usize),
}

fn edit_script(a: &[&str], b: &[&str]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > MAX_CELLS {
        return (0..n)
            .map(Op::Delete)
            .chain((0..m).map(Op::Insert))
            .collect();
    }
    let w = m + 1;
    let mut lcs = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * w + j] = if a[i] == b[j] {
                lcs[(i + 1) * w + j + 1] + 1
            } else {
                lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut ops = Vec::with_capacity(n + m);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            ops.push(Op::Equal(i));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
            ops.push(Op::Delete(i));
            i += 1;
        } else {
            ops.push(Op::Insert(j));
            j += 1;
        }
    }
    ops
}

/// `start,count` for one side of a hunk header; `before` is the number of that side's lines
/// preceding the hunk.
fn range(before: usize, count: usize) -> String {
    match count {
        0 => format!("{before},0"),
        1 => format!("{}", before + 1),
        _ => format!("{},{count}", before + 1),
    }
}

/// Unified diff of `before` to `after` by line; empty when they have the same lines.
pub fn unified(before: &str, after: &str) -> String {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let ops = edit_script(&a, &b);
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| !matches!(o, Op::Equal(_)))
        .map(|(k, _)| k)
        .collect();
    if changed.is_empty() {
        return String::new();
    }

    // Group changes separated by at most 2*CONTEXT equal lines into one hunk.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for &k in &changed {
        match groups.last_mut() {
            Some((_, last)) if k - *last - 1 <= 2 * CONTEXT => *last = k,
            _ => groups.push((k, k)),
        }
    }

    let mut out = String::new();
    for (first, last) in groups {
        let lo = first.saturating_sub(CONTEXT);
        let hi = (last + CONTEXT).min(ops.len() - 1);
        let (mut a_before, mut b_before) = (0, 0);
        for o in &ops[..lo] {
            match o {
                Op::Equal(_) => {
                    a_before += 1;
                    b_before += 1;
                }
                Op::Delete(_) => a_before += 1,
                Op::Insert(_) => b_before += 1,
            }
        }
        let (mut a_count, mut b_count) = (0, 0);
        let mut body = String::new();
        for o in &ops[lo..=hi] {
            match *o {
                Op::Equal(i) => {
                    a_count += 1;
                    b_count += 1;
                    body.push_str(&format!(" {}\n", a[i]));
                }
                Op::Delete(i) => {
                    a_count += 1;
                    body.push_str(&format!("-{}\n", a[i]));
                }
                Op::Insert(j) => {
                    b_count += 1;
                    body.push_str(&format!("+{}\n", b[j]));
                }
            }
        }
        out.push_str(&format!(
            "@@ -{} +{} @@\n{body}",
            range(a_before, a_count),
            range(b_before, b_count)
        ));
    }
    out
}
