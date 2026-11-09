//! Harness-native instruction files (spec §2.3): the files Claude Code and Codex would look for around a
//! seat's cwd, found by one discovery rule per harness.
//!
//! Owns: the per-harness discovery rules, the merge of one file picked by several harnesses into one entry,
//! and the `origin` / `note` / `env` labelling. Reports presence, size and mtime only.
//!
//! Must not: open a file for reading (metadata only, so an unreadable file is still listed), follow
//! `@`-imports, claim a listed file is loaded (the view says "present, not necessarily loaded"), or read the
//! agent's environment (user-level paths come from the inspector's own environment, hence `env: inspector`).
//!
//! Where new behavior goes: a new harness is a new `HarnessName` variant with its own discovery function
//! feeding [`Acc::add`]; the view never discovers files itself.
use crate::model::Timestamp;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HarnessFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub mtime: Option<Timestamp>,
    pub origin: Origin,
    pub harnesses: Vec<HarnessName>,
    pub note: Option<FileNote>,
    /// Set on user-level entries: their path comes from the inspecting process's environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<EnvSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Project,
    Ancestor,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessName {
    Claude,
    Codex,
}

/// When both notes apply (an `AGENTS.md` that Codex shadows and only Claude picks), `Shadowed` wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileNote {
    Shadowed,
    ClaudeOnlyWithoutClaudeMd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvSource {
    Inspector,
}

/// The user-level directories, from the inspector's environment (injected for tests).
#[derive(Debug, Clone, Default)]
pub struct UserDirs {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}

impl UserDirs {
    /// `CLAUDE_CONFIG_DIR` else `$HOME/.claude`; `CODEX_HOME` else `$HOME/.codex`. Read directly, not through
    /// the isolation-guarded setup helpers.
    pub fn from_env() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = var("HOME");
        UserDirs {
            claude: var("CLAUDE_CONFIG_DIR").or_else(|| home.as_ref().map(|h| h.join(".claude"))),
            codex: var("CODEX_HOME").or_else(|| home.as_ref().map(|h| h.join(".codex"))),
        }
    }
}

const CLAUDE_FILES: [&str; 2] = ["CLAUDE.md", "CLAUDE.local.md"];

/// Size and mtime of a regular file (following symlinks), without opening it.
fn stat(path: &Path) -> Option<(u64, Option<Timestamp>)> {
    let md = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((md.len(), md.modified().ok().map(Timestamp::from)))
}

fn exists(path: &Path) -> bool {
    stat(path).is_some()
}

#[derive(Default)]
struct Acc {
    files: BTreeMap<PathBuf, HarnessFile>,
    shadowed: BTreeSet<PathBuf>,
    claude_fallback: BTreeSet<PathBuf>,
}

impl Acc {
    /// Record `path` for `harness` (`None`: listed but picked by no harness, a shadowed file). A path that
    /// does not exist is skipped.
    fn add(&mut self, path: PathBuf, origin: Origin, harness: Option<HarnessName>) {
        let Some((bytes, mtime)) = stat(&path) else {
            return;
        };
        let e = self.files.entry(path.clone()).or_insert(HarnessFile {
            path,
            bytes,
            mtime,
            origin,
            harnesses: vec![],
            note: None,
            env: (origin == Origin::User).then_some(EnvSource::Inspector),
        });
        if let Some(h) = harness
            && !e.harnesses.contains(&h)
        {
            e.harnesses.push(h);
            e.harnesses.sort();
        }
    }

    fn finish(self) -> Vec<HarnessFile> {
        let Acc {
            files,
            shadowed,
            claude_fallback,
        } = self;
        let mut out: Vec<HarnessFile> = files
            .into_values()
            .map(|mut f| {
                f.note = if shadowed.contains(&f.path) {
                    Some(FileNote::Shadowed)
                } else if claude_fallback.contains(&f.path)
                    && !f.harnesses.contains(&HarnessName::Codex)
                {
                    Some(FileNote::ClaudeOnlyWithoutClaudeMd)
                } else {
                    None
                };
                f
            })
            .collect();
        out.sort_by(|a, b| (a.origin, &a.path).cmp(&(b.origin, &b.path)));
        out
    }
}

/// The nearest directory at or above `cwd` holding `.git` (a directory, or a file for a worktree); `cwd`
/// itself when there is none.
fn git_root(cwd: &Path) -> PathBuf {
    cwd.ancestors()
        .find(|d| d.join(".git").symlink_metadata().is_ok())
        .unwrap_or(cwd)
        .to_path_buf()
}

fn origin_of(dir: &Path, cwd: &Path) -> Origin {
    if dir == cwd {
        Origin::Project
    } else {
        Origin::Ancestor
    }
}

/// `*.md` files under `dir`, recursively. Directories are listed, never files opened; symlinked
/// directories are not entered.
fn rule_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => rule_files(&path, out),
            _ if path.extension().is_some_and(|e| e == "md") => out.push(path),
            _ => {}
        }
    }
}

fn discover_codex(acc: &mut Acc, cwd: &Path, user: &UserDirs) {
    let root = git_root(cwd);
    let mut chain: Vec<&Path> = cwd
        .ancestors()
        .take_while(|d| d.starts_with(&root))
        .collect();
    chain.reverse();
    for dir in chain {
        let origin = origin_of(dir, cwd);
        let (over, plain) = (dir.join("AGENTS.override.md"), dir.join("AGENTS.md"));
        if exists(&over) {
            acc.add(over, origin, Some(HarnessName::Codex));
            if exists(&plain) {
                acc.shadowed.insert(plain.clone());
                acc.add(plain, origin, None);
            }
        } else {
            acc.add(plain, origin, Some(HarnessName::Codex));
        }
    }
    if let Some(dir) = &user.codex {
        let over = dir.join("AGENTS.override.md");
        let pick = if exists(&over) {
            over
        } else {
            dir.join("AGENTS.md")
        };
        acc.add(pick, Origin::User, Some(HarnessName::Codex));
    }
}

fn discover_claude(acc: &mut Acc, cwd: &Path, user: &UserDirs) {
    let dirs: Vec<&Path> = cwd.ancestors().collect();
    let has_claude_md = dirs
        .iter()
        .any(|d| CLAUDE_FILES.iter().any(|f| exists(&d.join(f))));
    for dir in dirs {
        let origin = origin_of(dir, cwd);
        let claude = Some(HarnessName::Claude);
        for f in CLAUDE_FILES {
            acc.add(dir.join(f), origin, claude);
        }
        acc.add(dir.join(".claude").join("CLAUDE.md"), origin, claude);
        let mut rules = Vec::new();
        rule_files(&dir.join(".claude").join("rules"), &mut rules);
        for r in rules {
            acc.add(r, origin, claude);
        }
        if !has_claude_md {
            let agents = dir.join("AGENTS.md");
            if exists(&agents) {
                acc.claude_fallback.insert(agents.clone());
                acc.add(agents, origin, claude);
            }
        }
    }
    if let Some(dir) = &user.claude {
        acc.add(
            dir.join("CLAUDE.md"),
            Origin::User,
            Some(HarnessName::Claude),
        );
    }
}

/// Every harness-native instruction file present for a seat whose cwd is `cwd`, project files first.
pub fn discover(cwd: &Path, user: &UserDirs) -> Vec<HarnessFile> {
    let mut acc = Acc::default();
    discover_codex(&mut acc, cwd, user);
    discover_claude(&mut acc, cwd, user);
    acc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Tree {
        _t: tempfile::TempDir,
        root: PathBuf,
    }

    fn tree() -> Tree {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        Tree { _t: t, root }
    }

    impl Tree {
        fn put(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.root.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
    }

    fn find<'a>(fs: &'a [HarnessFile], p: &Path) -> &'a HarnessFile {
        fs.iter()
            .find(|f| f.path == p)
            .unwrap_or_else(|| panic!("{} not listed in {fs:?}", p.display()))
    }

    use HarnessName::{Claude, Codex};

    #[test]
    fn override_shadows_agents_md() {
        let t = tree();
        t.dir("proj/.git");
        let over = t.put("proj/AGENTS.override.md", "o");
        let plain = t.put("proj/AGENTS.md", "pp");
        let fs = discover(&t.dir("proj"), &UserDirs::default());
        assert_eq!(find(&fs, &over).harnesses, vec![Codex]);
        let shadowed = find(&fs, &plain);
        assert_eq!(shadowed.note, Some(FileNote::Shadowed));
        assert!(!shadowed.harnesses.contains(&Codex));
        assert_eq!(shadowed.bytes, 2);
        assert_eq!(find(&fs, &over).note, None);
    }

    #[test]
    fn codex_bounded_by_git_root() {
        let t = tree();
        let above = t.put("AGENTS.md", "above");
        t.dir("repo/.git");
        let root_md = t.put("repo/AGENTS.md", "root");
        let mid = t.put("repo/a/AGENTS.md", "mid");
        let cwd = t.dir("repo/a/b");
        let fs = discover(&cwd, &UserDirs::default());
        assert!(find(&fs, &root_md).harnesses.contains(&Codex));
        assert_eq!(find(&fs, &root_md).origin, Origin::Ancestor);
        assert!(find(&fs, &mid).harnesses.contains(&Codex));
        // Above the Git root: listed for Claude (no CLAUDE.md anywhere), never for Codex.
        assert!(!find(&fs, &above).harnesses.contains(&Codex));
    }

    #[test]
    fn claude_agents_md_fallback() {
        let t = tree();
        t.dir("repo/.git");
        let agents = t.put("repo/AGENTS.md", "a");
        let cwd = t.dir("repo/sub");
        // No CLAUDE.md anywhere: AGENTS.md is picked by both, so no note.
        let fs = discover(&cwd, &UserDirs::default());
        assert_eq!(find(&fs, &agents).harnesses, vec![Claude, Codex]);
        assert_eq!(find(&fs, &agents).note, None);
        // Above the Git root Codex does not pick it: claude-only.
        let above = t.put("AGENTS.md", "up");
        let fs = discover(&cwd, &UserDirs::default());
        assert_eq!(find(&fs, &above).harnesses, vec![Claude]);
        assert_eq!(
            find(&fs, &above).note,
            Some(FileNote::ClaudeOnlyWithoutClaudeMd)
        );
        // A CLAUDE.md above the cwd switches the fallback off everywhere.
        let claude = t.put("CLAUDE.md", "c");
        let fs = discover(&cwd, &UserDirs::default());
        assert_eq!(find(&fs, &claude).harnesses, vec![Claude]);
        assert!(find(&fs, &agents).harnesses == vec![Codex]);
        assert!(fs.iter().all(|f| f.path != above));
    }

    #[test]
    fn user_level_codex_override() {
        let t = tree();
        let cwd = t.dir("w");
        let codex = t.dir("home/.codex");
        let claude = t.dir("home/.claude");
        t.put("home/.codex/AGENTS.md", "plain");
        let over = t.put("home/.codex/AGENTS.override.md", "over");
        let cl = t.put("home/.claude/CLAUDE.md", "c");
        let user = UserDirs {
            claude: Some(claude),
            codex: Some(codex.clone()),
        };
        let fs = discover(&cwd, &user);
        let o = find(&fs, &over);
        assert_eq!(
            (o.origin, o.harnesses.clone(), o.env),
            (Origin::User, vec![Codex], Some(EnvSource::Inspector))
        );
        assert!(fs.iter().all(|f| f.path != codex.join("AGENTS.md")));
        let c = find(&fs, &cl);
        assert_eq!(
            (c.origin, c.env),
            (Origin::User, Some(EnvSource::Inspector))
        );
        assert!(
            fs.iter()
                .filter(|f| f.origin != Origin::User)
                .all(|f| f.env.is_none())
        );
    }

    #[test]
    fn content_never_opened() {
        let t = tree();
        let p = t.put("w/CLAUDE.md", "twelve bytes");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        let fs = discover(&t.dir("w"), &UserDirs::default());
        let f = find(&fs, &p);
        assert_eq!((f.bytes, f.origin), (12, Origin::Project));
        assert!(f.mtime.is_some());
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[test]
    fn claude_rules_recursive() {
        let t = tree();
        let cwd = t.dir("w/sub");
        let a = t.put("w/sub/.claude/rules/a.md", "a");
        let b = t.put("w/sub/.claude/rules/deep/b.md", "b");
        let skip = t.put("w/sub/.claude/rules/deep/c.txt", "c");
        let local = t.put("w/sub/CLAUDE.local.md", "l");
        let dot = t.put("w/.claude/CLAUDE.md", "d");
        let fs = discover(&cwd, &UserDirs::default());
        for p in [&a, &b, &local] {
            assert_eq!(find(&fs, p).harnesses, vec![Claude]);
            assert_eq!(find(&fs, p).origin, Origin::Project);
        }
        assert_eq!(find(&fs, &dot).origin, Origin::Ancestor);
        assert!(fs.iter().all(|f| f.path != skip));
        // Sorted project first, then ancestor.
        let origins: Vec<Origin> = fs.iter().map(|f| f.origin).collect();
        assert!(origins.windows(2).all(|w| w[0] <= w[1]));
    }
}
