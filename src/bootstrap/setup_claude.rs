//! `herdr-graph setup claude|codex [--uninstall]` (spec §9): an owned SessionStart hook group plus the `seat`
//! and `graph` skills, in `${CLAUDE_CONFIG_DIR:-~/.claude}` (`settings.json`) or `${CODEX_HOME:-~/.codex}`
//! (`hooks.json`, which has the same `hooks.SessionStart` shape). Both harnesses fire SessionStart on startup,
//! resume, clear and compact, so the `/seat` prompt comes back after every context reset.
//!
//! Ownership follows the herdr-threads pattern: the installed hook group is recognised by an installation
//! marker (a trailing shell comment in its command, so Claude's settings schema sees nothing unusual), and a
//! small manifest beside `settings.json` records what the installer did. Install is idempotent: a second run
//! changes nothing. Install never overwrites a rules file it did not write (it refuses instead), and
//! anything it would own that already exists byte-identical (hook group, ask entries, rules file) is recorded
//! as pre-existing: uninstall leaves those in place until a later run writes them. Uninstall removes what setup
//! added (and the two skill directories) and leaves pre-existing artifacts in place. When
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

/// How a harness asks the user before `herdr-graph apply` runs (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalGate {
    /// Claude: entries in settings.json `permissions.ask`.
    SettingsAsk,
    /// Codex: the owned `rules/herdr-graph.rules` file in `$CODEX_HOME`.
    RulesFile,
}

/// Where a harness keeps its hooks and skills.
pub struct Target {
    /// The hooks file inside the config dir.
    settings: &'static str,
    group: fn(&Path) -> Value,
    /// How this harness asks before `apply`.
    pub gate: ApprovalGate,
}

pub const CLAUDE: Target = Target {
    settings: "settings.json",
    group: claude_group,
    gate: ApprovalGate::SettingsAsk,
};

/// Codex trusts a new or changed hook only after an interactive review (`/hooks`); setup never writes trust.
pub const CODEX: Target = Target {
    settings: "hooks.json",
    group: codex_group,
    gate: ApprovalGate::RulesFile,
};

/// The owned Codex rules file, relative to `$CODEX_HOME`.
const RULES_FILE: &str = "rules/herdr-graph.rules";

/// The `permissions.ask` entries that make Claude ask before `apply`: the bare command and, when the hook
/// uses an absolute binary, that path in the hook's quoting.
pub fn ask_entries(binary: &Path) -> Vec<String> {
    let mut entries = vec!["Bash(herdr-graph apply:*)".to_owned()];
    if binary.is_absolute() {
        entries.push(format!(
            "Bash({} apply:*)",
            sh_quote(&binary.to_string_lossy())
        ));
    }
    entries
}

/// The owned Codex rules file. Syntax verified with `codex execpolicy check` (codex-cli 0.163.0): both
/// rules report a `prompt` decision for `herdr-graph apply ...` and for the absolute-path form.
pub fn rules_text(binary: &Path) -> String {
    let mut text = format!("# herdr-graph asks before apply {OWNER_MARKER}\n");
    text.push_str("prefix_rule(pattern=[\"herdr-graph\", \"apply\"], decision=\"prompt\")\n");
    if binary.is_absolute() {
        let path = serde_json::to_string(&*binary.to_string_lossy()).expect("a string serializes");
        text.push_str(&format!(
            "prefix_rule(pattern=[{path}, \"apply\"], decision=\"prompt\")\n"
        ));
    }
    text
}

/// An `ask` entry that guards `herdr-graph apply`, bare or by absolute path.
fn is_apply_rule(s: &str) -> bool {
    let Some(program) = s
        .strip_prefix("Bash(")
        .and_then(|r| r.strip_suffix(" apply:*)"))
    else {
        return false;
    };
    let program = program
        .strip_prefix('\'')
        .and_then(|p| p.strip_suffix('\''))
        .unwrap_or(program);
    Path::new(program).file_name() == Some(std::ffi::OsStr::new("herdr-graph"))
}

/// Whether `target`'s apply approval gate is present in `config`.
pub fn gate_installed(target: &Target, config: &Path) -> anyhow::Result<bool> {
    match target.gate {
        ApprovalGate::SettingsAsk => {
            let path = config.join(target.settings);
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
            };
            let doc = parse_settings(&path, &text)?;
            Ok(doc["permissions"]["ask"]
                .as_array()
                .is_some_and(|a| a.iter().filter_map(Value::as_str).any(is_apply_rule)))
        }
        ApprovalGate::RulesFile => Ok(std::fs::read_to_string(config.join(RULES_FILE))
            .is_ok_and(|t| t.contains("prefix_rule(") && t.contains("\"apply\""))),
    }
}

/// Whether `config` holds `target`'s owned SessionStart hook. A missing settings file means no.
pub fn owned_hook_installed(target: &Target, config: &Path) -> anyhow::Result<bool> {
    let path = config.join(target.settings);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let doc = parse_settings(&path, &text)?;
    Ok(doc["hooks"]["SessionStart"]
        .as_array()
        .is_some_and(|a| a.iter().any(is_owned)))
}

/// `${CLAUDE_CONFIG_DIR:-<home>/.claude}`.
pub fn config_dir(claude_config_dir: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let dir = claude_config_dir
        .map(Path::to_path_buf)
        .or_else(|| home.map(|h| h.join(".claude")))?;
    crate::herdr::isolation::tripwire(&dir, "setup_claude::config_dir");
    Some(dir)
}

/// `${CODEX_HOME:-<home>/.codex}`.
pub fn codex_dir(codex_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let dir = codex_home
        .map(Path::to_path_buf)
        .or_else(|| home.map(|h| h.join(".codex")))?;
    crate::herdr::isolation::tripwire(&dir, "setup_claude::codex_dir");
    Some(dir)
}

fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%".contains(c))
    {
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

/// The Codex hook command: only the `/seat` prompt, as `additionalContext` JSON. Codex session ids come from
/// Herdr, so there is no session report.
pub fn codex_hook_command(binary: &Path) -> String {
    let b = sh_quote(&binary.to_string_lossy());
    format!("{b} seat --hook-prompt --codex {OWNER_MARKER}")
}

fn claude_group(binary: &Path) -> Value {
    json!({ "matcher": "", "hooks": [{ "type": "command", "command": hook_command(binary), "timeout": 10 }] })
}

fn codex_group(binary: &Path) -> Value {
    json!({ "hooks": [{ "type": "command", "command": codex_hook_command(binary), "timeout": 10 }] })
}

fn is_owned(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks.iter().any(|h| {
            h["command"]
                .as_str()
                .is_some_and(|c| c.contains(OWNER_MARKER))
        })
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
    /// The apply approval gate.
    pub gate: HookChange,
    /// Skills written (new or changed); empty on a repeat run.
    pub skills_written: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallReport {
    pub hook_removed: bool,
    /// An owned hook group was left in place because it was already there before setup.
    pub hook_kept: bool,
    /// An apply approval entry or rules file this installer added was removed.
    pub gate_removed: bool,
    pub skills_removed: Vec<String>,
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What the installer recorded, so uninstall can restore exactly.
#[derive(Debug, Default, Clone, PartialEq)]
struct Manifest {
    /// `settings.json` text before the first install; None when the installer created the file.
    original: Option<String>,
    /// sha256 of `settings.json` as the installer last wrote it.
    installed_sha: String,
    /// False once settings.json was edited by someone else and then rewritten by a re-install.
    restorable: bool,
    created_skills_root: bool,
    /// `permissions.ask` entries this installer added (not ones that were already there).
    gate_entries: Vec<String>,
    /// The rules file was written by this installer.
    gate_file: bool,
    /// The installer created the `rules/` directory.
    created_rules_dir: bool,
    /// The owned hook group was present, byte-identical to what setup would install, before setup first
    /// ran, and no setup run has written it since; any run that installs or updates the group clears it.
    hook_preexisting: bool,
}

impl Manifest {
    fn read(path: &Path) -> Option<Self> {
        let v: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        Some(Self {
            original: v["original"].as_str().map(str::to_owned),
            installed_sha: v["installed_sha"].as_str()?.to_owned(),
            restorable: v["restorable"].as_bool().unwrap_or(false),
            created_skills_root: v["created_skills_root"].as_bool().unwrap_or(false),
            gate_entries: v["gate_entries"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            gate_file: v["gate_file"].as_bool().unwrap_or(false),
            created_rules_dir: v["created_rules_dir"].as_bool().unwrap_or(false),
            hook_preexisting: v["hook_preexisting"].as_bool().unwrap_or(false),
        })
    }

    fn write(&self, path: &Path) -> anyhow::Result<()> {
        let v = json!({
            "original": self.original, "installed_sha": self.installed_sha,
            "restorable": self.restorable, "created_skills_root": self.created_skills_root,
            "gate_entries": self.gate_entries, "gate_file": self.gate_file,
            "created_rules_dir": self.created_rules_dir,
            "hook_preexisting": self.hook_preexisting,
        });
        std::fs::write(path, serde_json::to_string_pretty(&v)? + "\n")
            .with_context(|| format!("writing {}", path.display()))
    }
}

fn pretty(doc: &Value) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(doc)? + "\n")
}

fn parse_settings(path: &Path, text: &str) -> anyhow::Result<Value> {
    let doc: Value = serde_json::from_str(text)
        .with_context(|| format!("{} is not valid JSON; not modified", path.display()))?;
    if !doc.is_object() {
        bail!("{} is not a JSON object; not modified", path.display());
    }
    Ok(doc)
}

/// The `hooks.SessionStart` array, created when absent. Errors when present with another type.
fn session_start_mut<'a>(doc: &'a mut Value, path: &Path) -> anyhow::Result<&'a mut Vec<Value>> {
    let obj = doc.as_object_mut().expect("checked to be an object");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().with_context(|| {
        format!(
            "`hooks` in {} is not an object; not modified",
            path.display()
        )
    })?;
    let arr = hooks.entry("SessionStart").or_insert_with(|| json!([]));
    arr.as_array_mut().with_context(|| {
        format!(
            "`hooks.SessionStart` in {} is not an array; not modified",
            path.display()
        )
    })
}

/// Install the owned Claude hook group and the two skills. Idempotent.
pub fn install(config: &Path, binary: &Path) -> anyhow::Result<InstallReport> {
    install_for(&CLAUDE, config, binary)
}

/// Install the owned hook group and the two skills for `target`. Idempotent.
pub fn install_for(target: &Target, config: &Path, binary: &Path) -> anyhow::Result<InstallReport> {
    std::fs::create_dir_all(config).with_context(|| format!("creating {}", config.display()))?;
    let settings = config.join(target.settings);
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
    let desired = (target.group)(binary);
    let arr = session_start_mut(&mut doc, &settings)?;
    let owned_at: Vec<usize> = arr
        .iter()
        .enumerate()
        .filter(|(_, g)| is_owned(g))
        .map(|(i, _)| i)
        .collect();
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
    let rules_owned = old_manifest.as_ref().is_some_and(|m| m.gate_file);

    // The apply approval gate, computed before anything is written.
    let mut added = Vec::new();
    let mut rules_write: Option<String> = None;
    let gate = match target.gate {
        ApprovalGate::SettingsAsk => {
            let obj = doc.as_object_mut().expect("checked to be an object");
            let permissions = obj.entry("permissions").or_insert_with(|| json!({}));
            let permissions = permissions.as_object_mut().with_context(|| {
                format!(
                    "`permissions` in {} is not an object; not modified",
                    settings.display()
                )
            })?;
            let ask = permissions.entry("ask").or_insert_with(|| json!([]));
            let ask = ask.as_array_mut().with_context(|| {
                format!(
                    "`permissions.ask` in {} is not an array; not modified",
                    settings.display()
                )
            })?;
            for entry in ask_entries(binary) {
                if !ask.iter().any(|v| v.as_str() == Some(entry.as_str())) {
                    ask.push(json!(entry));
                    added.push(entry);
                }
            }
            if added.is_empty() {
                HookChange::Unchanged
            } else {
                HookChange::Installed
            }
        }
        ApprovalGate::RulesFile => {
            let desired = rules_text(binary);
            let path = config.join(RULES_FILE);
            match std::fs::read_to_string(&path) {
                Ok(t) if t == desired => HookChange::Unchanged,
                Ok(_) if !rules_owned => bail!(
                    "{} already exists with different content and was not written by herdr-graph setup; \
                     not modified. Merge the apply prompt rules into it by hand, or remove or rename it \
                     and run setup codex again",
                    path.display()
                ),
                Ok(_) => {
                    rules_write = Some(desired);
                    HookChange::Updated
                }
                Err(_) => {
                    rules_write = Some(desired);
                    HookChange::Installed
                }
            }
        }
    };

    let first_install = old_manifest.is_none();
    let skills_root_existed = skills_root(config).exists();
    let settings_changed = hook != HookChange::Unchanged || !added.is_empty();
    let mut m = match &old_manifest {
        Some(m) => m.clone(),
        None => Manifest {
            original: before.clone(),
            restorable: true,
            hook_preexisting: hook == HookChange::Unchanged,
            ..Manifest::default()
        },
    };
    if settings_changed {
        let text = pretty(&doc)?;
        // Someone edited the file since we last wrote it: exact restoration is no longer honest.
        if old_manifest.is_some()
            && before.as_deref().map(|t| sha(t.as_bytes())) != Some(m.installed_sha.clone())
        {
            m.restorable = false;
        }
        std::fs::write(&settings, &text)
            .with_context(|| format!("writing {}", settings.display()))?;
        m.installed_sha = sha(text.as_bytes());
        if first_install && !skills_root_existed {
            m.created_skills_root = true;
        }
    } else if first_install {
        // Hook group found without a manifest (installed by hand or the manifest was lost): record what we can.
        let text = before.unwrap_or_default();
        m = Manifest {
            original: Some(text.clone()),
            installed_sha: sha(text.as_bytes()),
            hook_preexisting: hook == HookChange::Unchanged,
            ..Manifest::default()
        };
    }
    if hook != HookChange::Unchanged {
        m.hook_preexisting = false;
    }
    for entry in added {
        if !m.gate_entries.contains(&entry) {
            m.gate_entries.push(entry);
        }
    }
    if let Some(text) = rules_write {
        let path = config.join(RULES_FILE);
        let dir = path.parent().expect("the rules file has a parent");
        if !dir.exists() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            m.created_rules_dir = true;
        }
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        m.gate_file = true;
    }
    let skills_written = write_skills(config)?;
    if first_install || old_manifest.as_ref() != Some(&m) {
        m.write(&manifest_path)?;
    }
    Ok(InstallReport {
        hook,
        gate,
        skills_written,
    })
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

/// Remove what `install` added; pre-existing artifacts stay.
pub fn uninstall(config: &Path) -> anyhow::Result<UninstallReport> {
    uninstall_for(&CLAUDE, config)
}

/// Remove what `install_for(target, ..)` added; pre-existing artifacts stay.
pub fn uninstall_for(target: &Target, config: &Path) -> anyhow::Result<UninstallReport> {
    let settings = config.join(target.settings);
    let manifest_path = config.join(MANIFEST);
    let manifest = Manifest::read(&manifest_path);
    let current = std::fs::read_to_string(&settings).ok();
    let mut hook_removed = false;
    let mut gate_removed = false;
    let hook_preexisting = manifest.as_ref().is_some_and(|m| m.hook_preexisting);

    if let Some(text) = &current {
        let exact = manifest
            .as_ref()
            .is_some_and(|m| m.restorable && sha(text.as_bytes()) == m.installed_sha);
        if exact {
            let m = manifest.as_ref().expect("checked");
            match &m.original {
                Some(original) => std::fs::write(&settings, original)?,
                None => std::fs::remove_file(&settings)?,
            }
            hook_removed = !hook_preexisting;
            gate_removed = manifest
                .as_ref()
                .is_some_and(|m| !m.gate_entries.is_empty());
        } else {
            let mut doc = parse_settings(&settings, text)?;
            let had_owned = !hook_preexisting
                && doc["hooks"]["SessionStart"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(is_owned));
            let recorded: &[String] = manifest.as_ref().map_or(&[], |m| &m.gate_entries);
            if had_owned || !recorded.is_empty() {
                let obj = doc.as_object_mut().expect("checked to be an object");
                if had_owned {
                    let hooks = obj
                        .get_mut("hooks")
                        .and_then(Value::as_object_mut)
                        .expect("hooks exist when an owned group does");
                    let arr = hooks
                        .get_mut("SessionStart")
                        .and_then(Value::as_array_mut)
                        .expect("checked");
                    arr.retain(|g| !is_owned(g));
                    if arr.is_empty() {
                        hooks.remove("SessionStart");
                    }
                    if hooks.is_empty() {
                        obj.remove("hooks");
                    }
                }
                if let Some(permissions) = obj.get_mut("permissions").and_then(Value::as_object_mut)
                {
                    if let Some(ask) = permissions.get_mut("ask").and_then(Value::as_array_mut) {
                        for entry in recorded {
                            if let Some(i) = ask.iter().position(|v| v.as_str() == Some(entry)) {
                                ask.remove(i);
                                gate_removed = true;
                            }
                        }
                        if ask.is_empty() && gate_removed {
                            permissions.remove("ask");
                        }
                    }
                    if permissions.is_empty() && gate_removed {
                        obj.remove("permissions");
                    }
                }
                if had_owned || gate_removed {
                    if obj.is_empty() && manifest.as_ref().is_some_and(|m| m.original.is_none()) {
                        std::fs::remove_file(&settings)?;
                    } else {
                        std::fs::write(&settings, pretty(&doc)?)?;
                    }
                }
                hook_removed = had_owned;
            }
        }
    }

    if manifest.as_ref().is_some_and(|m| m.gate_file) {
        let path = config.join(RULES_FILE);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            gate_removed = true;
        }
        let dir = path.parent().expect("the rules file has a parent");
        if manifest.as_ref().is_some_and(|m| m.created_rules_dir)
            && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none())
        {
            let _ = std::fs::remove_dir(dir);
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
    let hook_kept = hook_preexisting
        && current
            .as_deref()
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
            .is_some_and(|d| {
                d["hooks"]["SessionStart"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(is_owned))
            });
    Ok(UninstallReport {
        hook_removed,
        hook_kept,
        gate_removed,
        skills_removed,
    })
}
