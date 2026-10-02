//! Byte-range coverage arithmetic (spec §8.1, §8.2): half-open `[start, end)` ranges, coalesced unions,
//! gaps, and newline alignment. All functions are pure and order-independent.
use crate::model::ByteRange;

/// Sorted union of `ranges` with overlapping and adjacent ranges coalesced; empty ranges are dropped.
pub fn union(ranges: &[ByteRange]) -> Vec<ByteRange> {
    let mut sorted: Vec<ByteRange> = ranges.iter().copied().filter(|r| !r.is_empty()).collect();
    sorted.sort();
    let mut out: Vec<ByteRange> = Vec::with_capacity(sorted.len());
    for r in sorted {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// `existing` plus `new`, as a sorted coalesced union. Coverage only ever grows: the result always contains
/// every byte of `existing` and of `new`, whatever order results arrive in.
pub fn merge_coverage(existing: &[ByteRange], new: ByteRange) -> Vec<ByteRange> {
    let mut all = existing.to_vec();
    all.push(new);
    union(&all)
}

/// The bytes of `requested` that `covered` does not contain, as a sorted coalesced list.
pub fn gaps(requested: &[ByteRange], covered: &[ByteRange]) -> Vec<ByteRange> {
    let covered = union(covered);
    let mut out = Vec::new();
    for req in union(requested) {
        let mut cursor = req.start;
        for c in &covered {
            if c.end <= cursor {
                continue;
            }
            if c.start >= req.end {
                break;
            }
            if c.start > cursor {
                out.push(ByteRange { start: cursor, end: c.start });
            }
            cursor = cursor.max(c.end);
            if cursor >= req.end {
                break;
            }
        }
        if cursor < req.end {
            out.push(ByteRange { start: cursor, end: req.end });
        }
    }
    out
}

/// The position just after the last `\n` at or before `end` (0 when there is none): the newline-aligned end
/// of a transcript whose current length is `end`.
pub fn align_end(file_bytes: &[u8], end: u64) -> u64 {
    let limit = usize::try_from(end).unwrap_or(usize::MAX).min(file_bytes.len());
    file_bytes[..limit].iter().rposition(|b| *b == b'\n').map_or(0, |i| i as u64 + 1)
}

/// Newline-aligned length of an open file of length `len`, reading only the tail (transcripts get large).
pub fn aligned_len(file: &mut std::fs::File, len: u64) -> std::io::Result<u64> {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: u64 = 8192;
    let mut end = len;
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let mut buf = vec![0u8; (end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buf)?;
        if let Some(i) = buf.iter().rposition(|b| *b == b'\n') {
            return Ok(start + i as u64 + 1);
        }
        end = start;
    }
    Ok(0)
}
