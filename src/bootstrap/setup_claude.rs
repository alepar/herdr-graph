//! `herdr-graph setup claude [--uninstall]` (spec §9): an owned SessionStart hook group plus the `seat` and
//! `graph` skills, in `${CLAUDE_CONFIG_DIR:-~/.claude}`.
//!
//! Ownership follows the herdr-threads pattern: the installed hook group is recognised by an installation
//! marker (a trailing shell comment in its command, so Claude's settings schema sees nothing unusual), and a
//! small manifest beside `settings.json` records what the installer did. Install is idempotent: a second run
//! changes nothing. Uninstall removes exactly the owned group and the two skill directories. When
//! `settings.json` is still byte-for-byte what the installer wrote, it is restored to its exact prior bytes
//! (or removed when the installer created it); when someone edited it since, only the owned group is taken
//! out of the parsed document and every other hook and setting stays.
use anyhow::{Context, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Marks the hook command as ours.
pub const OWNER_MARKER: &str = "# herdr-graph-owned";
const MANIFEST: &str = "herdr-graph-setup.json";
const SKILLS: &[(&str, &str)] = &[
    ("seat", include_str!("../../skills/seat/SKILL.md")),
    ("graph", include_str!("../../skills/graph/SKILL.md")),
];

/// `${CLAUDE_CONFIG_DIR:-<home>/.claude}`.
pub fn config_dir(claude_config_dir: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let dir = claude_config_dir.map(Path::to_path_buf).or_else(|| home.map(|h| h.join(".claude")))?;
    crate::herdr::isolation::tripwire(&dir, "setup_claude::config_dir");
    Some(dir)
}

fn sh_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@%".contains(c)) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The hook command: prompt `/seat` first (when this is a graph pane), then report the session without
/// waiting on the daemon. `seat --hook-prompt` does not read stdin, so `session-report` still gets the payload.
pub fn hook_command(binary: &Path) -> String {
    let b = sh_quote(&binary.to_string_lossy());
    format!("{b} seat --hook-prompt; {b} session-report --from-hook claude {OWNER_MARKER}")
}

fn owned_group(binary: &Path) -> Value {
    json!({ "matcher": "", "hooks": [{ "type": "command", "command": hook_command(binary), "timeout": 10 }] })
}

fn is_owned(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains(OWNER_MARKER)))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookChange {
    Installed,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub hook: HookChange,
    /// Skills written (new or changed); empty on a repeat run.
    pub skills_written: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallReport {
    pub hook_removed: bool,
    pub skills_removed: Vec<String>,
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// What the installer recorded, so uninstall can restore exactly.
#[derive(Debug, Default)]
struct Manifest {
    /// `settings.json` text before the first install; None when the installer created the file.
    original: Option<String>,
    /// sha256 of `settings.json` as the installer last wrote it.
    installed_sha: String,
    /// False once settings.json was edited by someone else and then rewritten by a re-install.
    restorable: bool,
    created_skills_root: bool,
}

impl Manifest {
    fn read(path: &Path) -> Option<Self> {
        let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        Some(Self {
            original: v["original"].as_str().map(str::to_owned),
            installed_sha: v["installed_sha"].as_str()?.to_owned(),
            restorable: v["restorable"].as_bool().unwrap_or(false),
            created_skills_root: v["created_skills_root"].as_bool().unwrap_or(false),
        })
    }

    fn write(&self, path: &Path) -> anyhow::Result<()> {
        let v = json!({
            "original": self.original, "installed_sha": self.installed_sha,
            "restorable": self.restorable, "created_skills_root": self.created_skills_root,
        });
        std::fs::write(path, serde_json::to_string_pretty(&v)? + "\n").with_context(|| format!("writing {}", path.display()))
    }
}

fn pretty(doc: &Value) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(doc)? + "\n")
}

fn parse_settings(path: &Path, text: &str) -> anyhow::Result<Value> {
    let doc: Value = serde_json::from_str(text).with_context(|| format!("{} is not valid JSON; not modified", path.display()))?;
    if !doc.is_object() {
        bail!("{} is not a JSON object; not modified", path.display());
    }
    Ok(doc)
}

/// The `hooks.SessionStart` array, created when absent. Errors when present with another type.
fn session_start_mut<'a>(doc: &'a mut Value, path: &Path) -> anyhow::Result<&'a mut Vec<Value>> {
    let obj = doc.as_object_mut().expect("checked to be an object");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().with_context(|| format!("`hooks` in {} is not an object; not modified", path.display()))?;
    let arr = hooks.entry("SessionStart").or_insert_with(|| json!([]));
    arr.as_array_mut().with_context(|| format!("`hooks.SessionStart` in {} is not an array; not modified", path.display()))
}

/// Install the owned hook group and the two skills. Idempotent.
pub fn install(config: &Path, binary: &Path) -> anyhow::Result<InstallReport> {
    std::fs::create_dir_all(config).with_context(|| format!("creating {}", config.display()))?;
    let settings = config.join("settings.json");
    let manifest_path = config.join(MANIFEST);
    let before = match std::fs::read_to_string(&settings) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("reading {}", settings.display())),
    };
    let mut doc = match &before {
        Some(t) => parse_settings(&settings, t)?,
        None => json!({}),
    };
    let desired = owned_group(binary);
    let arr = session_start_mut(&mut doc, &settings)?;
    let owned_at: Vec<usize> = arr.iter().enumerate().filter(|(_, g)| is_owned(g)).map(|(i, _)| i).collect();
    let hook = match owned_at.as_slice() {
        [] => {
            arr.push(desired);
            HookChange::Installed
        }
        [i] if arr[*i] == desired => HookChange::Unchanged,
        [first, rest @ ..] => {
            arr[*first] = desired;
            for i in rest.iter().rev() {
                arr.remove(*i);
            }
            HookChange::Updated
        }
    };

    let old_manifest = Manifest::read(&manifest_path);
    let first_install = old_manifest.is_none();
    let skills_root_existed = skills_root(config).exists();
    if hook != HookChange::Unchanged {
        let text = pretty(&doc)?;
        let mut m = match old_manifest {
            Some(mut m) => {
                // Someone edited the file since we last wrote it: exact restoration is no longer honest.
                if before.as_deref().map(|t| sha(t.as_bytes())) != Some(m.installed_sha.clone()) {
                    m.restorable = false;
                }
                m
            }
            None => Manifest { original: before.clone(), restorable: true, ..Manifest::default() },
        };
        std::fs::write(&settings, &text).with_context(|| format!("writing {}", settings.display()))?;
        m.installed_sha = sha(text.as_bytes());
        if first_install && !skills_root_existed {
            m.created_skills_root = true;
        }
        let skills_written = write_skills(config)?;
        m.write(&manifest_path)?;
        return Ok(InstallReport { hook, skills_written });
    }
    let skills_written = write_skills(config)?;
    if first_install {
        // Hook group found without a manifest (installed by hand or the manifest was lost): record what we can.
        let text = before.unwrap_or_default();
        Manifest { original: Some(text.clone()), installed_sha: sha(text.as_bytes()), restorable: false, created_skills_root: false }
            .write(&manifest_path)?;
    }
    Ok(InstallReport { hook, skills_written })
}

fn skills_root(config: &Path) -> PathBuf {
    config.join("skills")
}

fn write_skills(config: &Path) -> anyhow::Result<Vec<String>> {
    let mut written = Vec::new();
    for (name, content) in SKILLS {
        let dir = skills_root(config).join(name);
        let file = dir.join("SKILL.md");
        if std::fs::read_to_string(&file).ok().as_deref() == Some(content) {
            continue;
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        std::fs::write(&file, content).with_context(|| format!("writing {}", file.display()))?;
        written.push((*name).to_owned());
    }
    Ok(written)
}

/// Remove exactly what `install` added.
pub fn uninstall(config: &Path) -> anyhow::Result<UninstallReport> {
    let settings = config.join("settings.json");
    let manifest_path = config.join(MANIFEST);
    let manifest = Manifest::read(&manifest_path);
    let current = std::fs::read_to_string(&settings).ok();
    let mut hook_removed = false;

    if let Some(text) = &current {
        let exact = manifest.as_ref().is_some_and(|m| m.restorable && sha(text.as_bytes()) == m.installed_sha);
        if exact {
            let m = manifest.as_ref().expect("checked");
            match &m.original {
                Some(original) => std::fs::write(&settings, original)?,
                None => std::fs::remove_file(&settings)?,
            }
            hook_removed = true;
        } else {
            let mut doc = parse_settings(&settings, text)?;
            let had_owned = doc["hooks"]["SessionStart"].as_array().is_some_and(|a| a.iter().any(is_owned));
            if had_owned {
                let obj = doc.as_object_mut().expect("checked to be an object");
                let hooks = obj.get_mut("hooks").and_then(Value::as_object_mut).expect("hooks exist when an owned group does");
                let arr = hooks.get_mut("SessionStart").and_then(Value::as_array_mut).expect("checked");
                arr.retain(|g| !is_owned(g));
                if arr.is_empty() {
                    hooks.remove("SessionStart");
                }
                if hooks.is_empty() {
                    obj.remove("hooks");
                }
                if obj.is_empty() && manifest.as_ref().is_some_and(|m| m.original.is_none()) {
                    std::fs::remove_file(&settings)?;
                } else {
                    std::fs::write(&settings, pretty(&doc)?)?;
                }
                hook_removed = true;
            }
        }
    }

    let mut skills_removed = Vec::new();
    for (name, _) in SKILLS {
        let dir = skills_root(config).join(name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
            skills_removed.push((*name).to_owned());
        }
    }
    let root = skills_root(config);
    if manifest.as_ref().is_some_and(|m| m.created_skills_root)
        && root.is_dir()
        && std::fs::read_dir(&root).is_ok_and(|mut d| d.next().is_none())
    {
        let _ = std::fs::remove_dir(&root);
    }
    if manifest.is_some() {
        std::fs::remove_file(&manifest_path)?;
    }
    Ok(UninstallReport { hook_removed, skills_removed })
}
