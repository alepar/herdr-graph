//! Instruction sources outside templates (spec §2.1, §2.2a): the closed set of scopes and files, their
//! paths at a revision, the affected seats, the instruction-vs-data classification and the admissibility
//! predicate. Never accepts a caller-supplied path.
use super::layout;
use super::tree::TreeRead;
use crate::model::{AnyId, Lifecycle, SeatId, TeamspaceId};
use crate::ports::store::{EntryKind, RepoPath, StoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionScope {
    Global,
    Teamspace(TeamspaceId),
    Seat(SeatId),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum InstructionFile {
    Rule(String),
    AgentsMd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Teamspace,
    Seat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderKind {
    Root,
    Teamspace,
    Seat,
    Clone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathClass {
    Instruction,
    Record,
    Ignored,
    Data,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionRef {
    pub scope: ScopeKind,
    pub target: Option<String>,
    pub file: Option<InstructionFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionTarget {
    pub scope: InstructionScope,
    pub file: InstructionFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedFile {
    pub path: RepoPath,
    pub rel: String,
    pub name: Result<InstructionFile, Inadmissible>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedSeat {
    pub id: SeatId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstructionError {
    #[error("usage: {0}")]
    Usage(String),
    #[error(
        "invalid instruction file name {0:?}: rule names match [a-z0-9][a-z0-9_-]* and live directly in rules/"
    )]
    BadName(String),
    #[error("{file} is not an instruction file of the {scope} scope; allowed: {allowed}")]
    NotAllowed {
        scope: String,
        file: String,
        allowed: String,
    },
    #[error("no {what} {target:?}")]
    UnknownTarget { what: String, target: String },
    #[error("retired_object: {what} {target} is retired")]
    Retired { what: String, target: String },
    #[error("{0}")]
    Ambiguous(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl PartialEq for StoreError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}
impl Eq for StoreError {}
impl Clone for StoreError {
    fn clone(&self) -> Self {
        match self {
            Self::NoInstance(p) => Self::NoInstance(p.clone()),
            Self::UnknownCommit(s) => Self::UnknownCommit(s.clone()),
            Self::InvalidPath(s) => Self::InvalidPath(s.clone()),
            Self::Corrupt { path, reason } => Self::Corrupt {
                path: path.clone(),
                reason: reason.clone(),
            },
            Self::Git(s) => Self::Git(s.clone()),
            Self::Io(e) => Self::Io(std::io::Error::new(e.kind(), e.to_string())),
        }
    }
}

/// Why a name or text is not admissible as an instruction source; `Display` is the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inadmissible {
    pub reason: String,
}

impl std::fmt::Display for Inadmissible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl ScopeKind {
    pub fn word(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Teamspace => "teamspace",
            Self::Seat => "seat",
        }
    }
    fn allowed(self) -> &'static str {
        match self {
            Self::Seat => "AGENTS.md, rules/<name>.md",
            Self::Global | Self::Teamspace => "rules/<name>.md",
        }
    }
}

const AGENTS_MD: &str = "AGENTS.md";
const RULES_PREFIX: &str = "rules/";

/// `[a-z0-9][a-z0-9_-]*`
fn valid_rule_name(n: &str) -> bool {
    let mut chars = n.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// `rules/<n>.md` with a valid, un-nested name.
fn rule_from_rel(rel: &str) -> Option<InstructionFile> {
    let name = rel.strip_prefix(RULES_PREFIX)?.strip_suffix(".md")?;
    valid_rule_name(name).then(|| InstructionFile::Rule(name.to_owned()))
}

impl InstructionFile {
    /// `AGENTS.md` | `rules/<n>.md`, checked against what the scope allows.
    pub fn parse(scope: ScopeKind, word: &str) -> Result<Self, InstructionError> {
        if word == AGENTS_MD && scope == ScopeKind::Seat {
            return Ok(Self::AgentsMd);
        }
        if word.starts_with(RULES_PREFIX) {
            return rule_from_rel(word).ok_or_else(|| InstructionError::BadName(word.to_owned()));
        }
        Err(InstructionError::NotAllowed {
            scope: scope.word().to_owned(),
            file: word.to_owned(),
            allowed: scope.allowed().to_owned(),
        })
    }

    pub fn rel(&self) -> String {
        match self {
            Self::AgentsMd => AGENTS_MD.to_owned(),
            Self::Rule(n) => format!("{RULES_PREFIX}{n}.md"),
        }
    }
}

impl InstructionScope {
    pub fn kind(&self) -> ScopeKind {
        match self {
            Self::Global => ScopeKind::Global,
            Self::Teamspace(_) => ScopeKind::Teamspace,
            Self::Seat(_) => ScopeKind::Seat,
        }
    }

    /// The object an instruction effect is recorded against (the graph itself for Global).
    pub fn object_id(&self, tree: &dyn TreeRead) -> Result<AnyId, InstructionError> {
        Ok(match self {
            Self::Global => layout::graph_id(tree)?.to_any(),
            Self::Teamspace(id) => id.to_any(),
            Self::Seat(id) => id.to_any(),
        })
    }
}

/// `<scope> [<target>] <file>` (file required: plans) or `<scope> [<target>] [<file>]` (file optional: show).
pub fn parse_words(
    words: &[String],
    file_required: bool,
) -> Result<InstructionRef, InstructionError> {
    let usage = |m: &str| InstructionError::Usage(m.to_owned());
    let (first, rest) = words
        .split_first()
        .ok_or_else(|| usage("expected global|teamspace|seat"))?;
    let scope = match first.as_str() {
        "global" => ScopeKind::Global,
        "teamspace" => ScopeKind::Teamspace,
        "seat" => ScopeKind::Seat,
        other => {
            return Err(usage(&format!(
                "unknown instruction scope {other:?}; expected global|teamspace|seat"
            )));
        }
    };
    let (target, file_words) = match scope {
        ScopeKind::Global => (None, rest),
        _ => match rest.split_first() {
            Some((t, f)) => (Some(t.clone()), f),
            None => return Err(usage(&format!("{} needs a name or id", scope.word()))),
        },
    };
    let file = match file_words {
        [] if file_required => {
            return Err(usage("a file is required: AGENTS.md or rules/<name>.md"));
        }
        [] => None,
        [f] => Some(InstructionFile::parse(scope, f)?),
        _ => return Err(usage("too many words")),
    };
    Ok(InstructionRef {
        scope,
        target,
        file,
    })
}

/// Pick one object among name matches `(id, live)`: one live → ok; several live → Ambiguous; only retired → Retired.
fn pick<T: Clone>(
    what: &str,
    target: &str,
    matches: Vec<(T, String, bool)>,
) -> Result<T, InstructionError> {
    let live: Vec<_> = matches.iter().filter(|m| m.2).collect();
    match (live.as_slice(), matches.first()) {
        ([one], _) => Ok(one.0.clone()),
        ([], Some(_)) => Err(InstructionError::Retired {
            what: what.to_owned(),
            target: target.to_owned(),
        }),
        ([], None) => Err(InstructionError::UnknownTarget {
            what: what.to_owned(),
            target: target.to_owned(),
        }),
        (many, _) => {
            let ids: Vec<&str> = many.iter().map(|m| m.1.as_str()).collect();
            Err(InstructionError::Ambiguous(format!(
                "{what} name {target:?} is ambiguous: {}",
                ids.join(", ")
            )))
        }
    }
}

/// Resolve the target (name among live objects, or id) at this revision; retired → Retired.
pub fn resolve_scope(
    tree: &dyn TreeRead,
    scope: ScopeKind,
    target: Option<&str>,
) -> Result<InstructionScope, InstructionError> {
    let need = |what: &str| {
        target.ok_or_else(|| InstructionError::Usage(format!("{what} needs a name or id")))
    };
    let unknown = |what: &str, t: &str| InstructionError::UnknownTarget {
        what: what.to_owned(),
        target: t.to_owned(),
    };
    let retired = |what: &str, t: &str| InstructionError::Retired {
        what: what.to_owned(),
        target: t.to_owned(),
    };
    match scope {
        ScopeKind::Global => match target {
            None => Ok(InstructionScope::Global),
            Some(_) => Err(InstructionError::Usage("global takes no target".into())),
        },
        ScopeKind::Teamspace => {
            let t = need("teamspace")?;
            let all = layout::list_teamspaces(tree)?;
            if let Ok(id) = TeamspaceId::parse(t) {
                let (_, rec) = all
                    .iter()
                    .find(|(_, r)| r.id == id)
                    .ok_or_else(|| unknown("teamspace", t))?;
                return if rec.lifecycle == Lifecycle::Retired {
                    Err(retired("teamspace", t))
                } else {
                    Ok(InstructionScope::Teamspace(id))
                };
            }
            let matches = all
                .iter()
                .filter(|(_, r)| r.name == t)
                .map(|(_, r)| {
                    (
                        r.id.clone(),
                        r.id.to_string(),
                        r.lifecycle != Lifecycle::Retired,
                    )
                })
                .collect();
            pick("teamspace", t, matches).map(InstructionScope::Teamspace)
        }
        ScopeKind::Seat => {
            let t = need("seat")?;
            let all = layout::all_seats(tree)?;
            if let Ok(id) = SeatId::parse(t) {
                let (_, rec) = all
                    .iter()
                    .find(|(_, r)| r.id == id)
                    .ok_or_else(|| unknown("seat", t))?;
                return if rec.lifecycle == Lifecycle::Retired {
                    Err(retired("seat", t))
                } else {
                    Ok(InstructionScope::Seat(id))
                };
            }
            let matches = all
                .iter()
                .filter(|(_, r)| r.name == t)
                .map(|(_, r)| {
                    (
                        r.id.clone(),
                        r.id.to_string(),
                        r.lifecycle != Lifecycle::Retired,
                    )
                })
                .collect();
            pick("seat", t, matches).map(InstructionScope::Seat)
        }
    }
}

pub fn scope_folder(
    tree: &dyn TreeRead,
    scope: &InstructionScope,
) -> Result<RepoPath, InstructionError> {
    let (what, id) = match scope {
        InstructionScope::Global => return Ok(RepoPath::root()),
        InstructionScope::Teamspace(id) => ("teamspace", id.to_any()),
        InstructionScope::Seat(id) => ("seat", id.to_any()),
    };
    layout::locate(tree, &id)?
        .map(|l| l.folder)
        .ok_or_else(|| InstructionError::UnknownTarget {
            what: what.to_owned(),
            target: id.to_string(),
        })
}

pub fn path(tree: &dyn TreeRead, t: &InstructionTarget) -> Result<RepoPath, InstructionError> {
    Ok(scope_folder(tree, &t.scope)?.join(&t.file.rel())?)
}

fn basename(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// Every file below `dir`, as `(path, rel-to-base)`; dotfiles are skipped.
fn walk(
    tree: &dyn TreeRead,
    dir: &RepoPath,
    rel: &str,
    out: &mut Vec<(RepoPath, String)>,
) -> Result<(), StoreError> {
    for e in tree.list_dir(dir)? {
        let child = dir.join(&e.name)?;
        let child_rel = format!("{rel}/{}", e.name);
        match e.kind {
            EntryKind::Dir => walk(tree, &child, &child_rel, out)?,
            _ if e.name.starts_with('.') => {}
            _ => out.push((child, child_rel)),
        }
    }
    Ok(())
}

/// Every non-dotfile in the scope's instruction locations (rules/ recursively; AGENTS.md for Seat), sorted by rel.
pub fn list_files(
    tree: &dyn TreeRead,
    scope: &InstructionScope,
) -> Result<Vec<ListedFile>, InstructionError> {
    let folder = scope_folder(tree, scope)?;
    let kind = scope.kind();
    let mut found = Vec::new();
    if kind == ScopeKind::Seat {
        let p = folder.join(AGENTS_MD)?;
        if tree.read_file(&p)?.is_some() {
            found.push((p, AGENTS_MD.to_owned()));
        }
    }
    walk(tree, &folder.join("rules")?, "rules", &mut found)?;
    found.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(found
        .into_iter()
        .map(|(path, rel)| ListedFile {
            name: check_name(kind, &rel),
            path,
            rel,
        })
        .collect())
}

pub fn affected_seats(
    tree: &dyn TreeRead,
    scope: &InstructionScope,
) -> Result<Vec<AffectedSeat>, InstructionError> {
    let live = |l: Lifecycle| l != Lifecycle::Retired;
    let seats = layout::all_seats(tree)?;
    let mut out: Vec<AffectedSeat> = match scope {
        InstructionScope::Global => {
            let live_ts: Vec<TeamspaceId> = layout::list_teamspaces(tree)?
                .into_iter()
                .filter(|(_, t)| live(t.lifecycle))
                .map(|(_, t)| t.id)
                .collect();
            seats
                .into_iter()
                .filter(|(_, s)| live(s.lifecycle) && live_ts.contains(&s.teamspace))
                .map(|(_, s)| AffectedSeat {
                    id: s.id,
                    name: s.name,
                })
                .collect()
        }
        InstructionScope::Teamspace(id) => seats
            .into_iter()
            .filter(|(_, s)| live(s.lifecycle) && &s.teamspace == id)
            .map(|(_, s)| AffectedSeat {
                id: s.id,
                name: s.name,
            })
            .collect(),
        InstructionScope::Seat(id) => vec![
            seats
                .into_iter()
                .find(|(_, s)| &s.id == id)
                .map(|(_, s)| AffectedSeat {
                    id: s.id,
                    name: s.name,
                })
                .ok_or_else(|| InstructionError::UnknownTarget {
                    what: "seat".to_owned(),
                    target: id.to_string(),
                })?,
        ],
    };
    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    Ok(out)
}

/// Dotfiles are ignored; record files are records; under a rules/ folder or a scope AGENTS.md nothing is
/// ever data; in a clone folder everything else is data.
pub fn classify(folder: FolderKind, rel: &str) -> PathClass {
    let base = basename(rel);
    if base.starts_with('.') {
        return PathClass::Ignored;
    }
    if layout::RECORD_FILES.contains(&base) {
        return PathClass::Record;
    }
    match folder {
        FolderKind::Clone => PathClass::Data,
        FolderKind::Root | FolderKind::Teamspace | FolderKind::Seat => {
            if rel == AGENTS_MD || rel.starts_with(RULES_PREFIX) {
                PathClass::Instruction
            } else {
                PathClass::Data
            }
        }
    }
}

pub fn is_instruction_source(folder: FolderKind, rel: &str) -> bool {
    classify(folder, rel) == PathClass::Instruction
}

/// The §2.2a name clause for a file found in a scope's instruction location (rel relative to the scope folder).
pub fn check_name(scope: ScopeKind, rel: &str) -> Result<InstructionFile, Inadmissible> {
    if rel == AGENTS_MD {
        return if scope == ScopeKind::Seat {
            Ok(InstructionFile::AgentsMd)
        } else {
            Err(Inadmissible {
                reason: format!(
                    "{AGENTS_MD} is not an instruction file of the {} scope",
                    scope.word()
                ),
            })
        };
    }
    rule_from_rel(rel).ok_or_else(|| Inadmissible {
        reason: format!(
            "invalid instruction file name {rel:?}: rule names match [a-z0-9][a-z0-9_-]* and live directly in rules/"
        ),
    })
}

/// Invisible and format code points (inclusive ranges) that are never admissible.
const INVISIBLE: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x061C, 0x061C),
    (0x115F, 0x1160),
    (0x180B, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF9, 0xFFFB),
    (0xE0000, 0xE007F),
    (0xE0100, 0xE01EF),
];

/// An explicit approximation of Unicode Emoji=Yes (std has no property API and no crate is added): the
/// bases a U+FE0E / U+FE0F presentation selector may follow.
const EMOJI_BASE: &[(u32, u32)] = &[
    (0x00A9, 0x00A9),
    (0x00AE, 0x00AE),
    (0x203C, 0x203C),
    (0x2049, 0x2049),
    (0x2122, 0x2122),
    (0x2139, 0x2139),
    (0x2194, 0x2199),
    (0x21A9, 0x21AA),
    (0x231A, 0x231B),
    (0x2328, 0x2328),
    (0x23CF, 0x23CF),
    (0x23E9, 0x23F3),
    (0x23F8, 0x23FA),
    (0x24C2, 0x24C2),
    (0x25AA, 0x25AB),
    (0x25B6, 0x25B6),
    (0x25C0, 0x25C0),
    (0x25FB, 0x25FE),
    (0x2600, 0x27BF),
    (0x2934, 0x2935),
    (0x2B05, 0x2B07),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x3030, 0x3030),
    (0x303D, 0x303D),
    (0x3297, 0x3297),
    (0x3299, 0x3299),
    (0x1F000, 0x1FAFF),
];

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    ranges
        .iter()
        .any(|&(lo, hi)| (lo..=hi).contains(&(c as u32)))
}

/// The §2.2a content clause, callable on its own (template AGENTS.md, responsibility, section bodies).
/// U+FE0E and U+FE0F pass only directly after an emoji base, or after a keycap base (`0-9 # *`) that the
/// selector joins to U+20E3.
pub fn check_content(bytes: &[u8]) -> Result<&str, Inadmissible> {
    let text = std::str::from_utf8(bytes).map_err(|e| Inadmissible {
        reason: format!("not valid UTF-8 (at byte {})", e.valid_up_to()),
    })?;
    let mut line = 1usize;
    let mut chars = text.chars().peekable();
    let mut prev: Option<char> = None;
    while let Some(c) = chars.next() {
        let before = prev.replace(c);
        let bad = |what: &str| Inadmissible {
            reason: format!("{what} U+{:04X} at line {line}", c as u32),
        };
        match c {
            '\n' => line += 1,
            '\t' => {}
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => {
                return Err(Inadmissible {
                    reason: format!("bare carriage return (U+000D) at line {line}"),
                });
            }
            c if c.is_control() => return Err(bad("control character")),
            '\u{FE0E}' | '\u{FE0F}'
                if before.is_some_and(|p| in_ranges(p, EMOJI_BASE))
                    || (before.is_some_and(|p| matches!(p, '0'..='9' | '#' | '*'))
                        && chars.peek() == Some(&'\u{20E3}')) => {}
            c if in_ranges(c, INVISIBLE) => {
                return Err(bad("invisible or format character"));
            }
            _ => {}
        }
    }
    Ok(text)
}
