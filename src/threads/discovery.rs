//! Where the herdr-threads state directory is (bead hg-zmi.46). Pure: every input is injected.
use std::path::{Path, PathBuf};

pub const THREADS_PLUGIN_ID: &str = "herdr-threads";
const GRAPH_PLUGIN_ID: &str = "herdr-graph";

#[derive(Debug, Clone, Default)]
pub struct DiscoveryInputs {
    /// `HERDR_GRAPH_THREADS_STATE_DIR` (explicit override).
    pub env_state_dir: Option<PathBuf>,
    /// `threads_state_dir` from config.toml (see `config::read_threads_state_dir`).
    pub config_state_dir: Option<PathBuf>,
    /// `HERDR_PLUGIN_STATE_DIR` as herdr-graph sees it (its own plugin state dir).
    pub own_plugin_state_dir: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    EnvVar,
    Config,
    PluginSibling,
    XdgDefault,
    HomeDefault,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Source::EnvVar => "HERDR_GRAPH_THREADS_STATE_DIR",
            Source::Config => "threads_state_dir in config.toml",
            Source::PluginSibling => "sibling of the herdr-graph plugin state dir",
            Source::XdgDefault => "$XDG_STATE_HOME default",
            Source::HomeDefault => "~/.local/state default",
        })
    }
}

/// Every candidate state dir trips the isolation guard before it is probed.
fn trip(p: &Path) {
    crate::herdr::isolation::tripwire(p, "threads state dir candidate");
}

fn absolute(p: &Option<PathBuf>) -> Option<&Path> {
    p.as_deref()
        .filter(|p| p.is_absolute() && !p.as_os_str().is_empty())
}

impl DiscoveryInputs {
    /// The only function that touches the process environment (empty values count as unset).
    pub fn from_process(config_state_dir: Option<PathBuf>) -> Self {
        let var = |n: &str| {
            std::env::var_os(n)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            env_state_dir: var("HERDR_GRAPH_THREADS_STATE_DIR"),
            config_state_dir,
            own_plugin_state_dir: var("HERDR_PLUGIN_STATE_DIR"),
            xdg_state_home: var("XDG_STATE_HOME"),
            home: var("HOME"),
        }
    }
}

/// Ok(None): nothing found (threads not installed / never started). Err: ambiguous (both defaults exist).
pub fn resolve_state_dir(i: &DiscoveryInputs) -> Result<Option<(PathBuf, Source)>, String> {
    if let Some(p) = absolute(&i.env_state_dir) {
        trip(p);
        return Ok(Some((p.to_path_buf(), Source::EnvVar)));
    }
    if let Some(p) = absolute(&i.config_state_dir) {
        trip(p);
        return Ok(Some((p.to_path_buf(), Source::Config)));
    }
    if let Some(own) = absolute(&i.own_plugin_state_dir)
        && own.file_name().is_some_and(|n| n == GRAPH_PLUGIN_ID)
        && let Some(parent) = own.parent()
    {
        let sibling = parent.join(THREADS_PLUGIN_ID);
        trip(&sibling);
        if sibling.is_dir() {
            return Ok(Some((sibling, Source::PluginSibling)));
        }
    }
    let xdg = absolute(&i.xdg_state_home)
        .map(|x| x.join("herdr/plugins").join(THREADS_PLUGIN_ID))
        .filter(|p| {
            trip(p);
            p.is_dir()
        });
    let home = absolute(&i.home)
        .map(|h| h.join(".local/state/herdr/plugins").join(THREADS_PLUGIN_ID))
        .filter(|p| {
            trip(p);
            p.is_dir()
        });
    match (xdg, home) {
        (Some(x), Some(h)) if x != h => Err(format!(
            "both {} and {} exist; set threads_state_dir in config.toml",
            x.display(),
            h.display()
        )),
        (Some(x), _) => Ok(Some((x, Source::XdgDefault))),
        (None, Some(h)) => Ok(Some((h, Source::HomeDefault))),
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(p: &Path) -> PathBuf {
        std::fs::create_dir_all(p).unwrap();
        p.to_path_buf()
    }

    #[test]
    fn env_override_wins() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().to_path_buf();
        mk(&home.join(".local/state/herdr/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            env_state_dir: Some("/abs/env".into()),
            config_state_dir: Some("/abs/cfg".into()),
            home: Some(home),
            ..Default::default()
        };
        assert_eq!(
            resolve_state_dir(&i).unwrap(),
            Some((PathBuf::from("/abs/env"), Source::EnvVar))
        );
    }

    #[test]
    fn config_key_before_defaults() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().to_path_buf();
        mk(&home.join(".local/state/herdr/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            config_state_dir: Some("/abs/cfg".into()),
            home: Some(home),
            ..Default::default()
        };
        assert_eq!(
            resolve_state_dir(&i).unwrap(),
            Some((PathBuf::from("/abs/cfg"), Source::Config))
        );
    }

    #[test]
    fn plugin_sibling_of_herdr_graph_state_dir() {
        let t = tempfile::tempdir().unwrap();
        let own = mk(&t.path().join("herdr/plugins/herdr-graph"));
        let sibling = mk(&t.path().join("herdr/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            own_plugin_state_dir: Some(own.clone()),
            ..Default::default()
        };
        let (dir, src) = resolve_state_dir(&i).unwrap().unwrap();
        assert_eq!((dir.clone(), src), (sibling, Source::PluginSibling));
        assert_ne!(dir, own);
        // A plugin dir with another name is never mistaken for the graph's own dir.
        let other = mk(&t.path().join("x/plugins/other"));
        mk(&t.path().join("x/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            own_plugin_state_dir: Some(other),
            ..Default::default()
        };
        assert_eq!(resolve_state_dir(&i).unwrap(), None);
    }

    #[test]
    fn xdg_default_when_present() {
        let t = tempfile::tempdir().unwrap();
        let xdg = mk(&t.path().join("xdg"));
        let want = mk(&xdg.join("herdr/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            xdg_state_home: Some(xdg),
            home: Some(t.path().join("home")),
            ..Default::default()
        };
        assert_eq!(
            resolve_state_dir(&i).unwrap(),
            Some((want, Source::XdgDefault))
        );
    }

    #[test]
    fn home_default_when_present() {
        let t = tempfile::tempdir().unwrap();
        let want = mk(&t.path().join(".local/state/herdr/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            home: Some(t.path().to_path_buf()),
            ..Default::default()
        };
        assert_eq!(
            resolve_state_dir(&i).unwrap(),
            Some((want, Source::HomeDefault))
        );
    }

    #[test]
    fn both_defaults_is_ambiguous() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home");
        mk(&home.join(".local/state/herdr/plugins/herdr-threads"));
        let xdg = mk(&t.path().join("xdg"));
        mk(&xdg.join("herdr/plugins/herdr-threads"));
        let i = DiscoveryInputs {
            xdg_state_home: Some(xdg),
            home: Some(home),
            ..Default::default()
        };
        let err = resolve_state_dir(&i).unwrap_err();
        assert!(err.contains("threads_state_dir"), "{err}");
    }

    #[test]
    fn nothing_found_is_none() {
        let t = tempfile::tempdir().unwrap();
        let i = DiscoveryInputs {
            home: Some(t.path().to_path_buf()),
            ..Default::default()
        };
        assert_eq!(resolve_state_dir(&i).unwrap(), None);
    }

    #[test]
    fn relative_paths_ignored() {
        let i = DiscoveryInputs {
            env_state_dir: Some("rel/env".into()),
            config_state_dir: Some("".into()),
            own_plugin_state_dir: Some("rel/herdr-graph".into()),
            xdg_state_home: Some("rel/xdg".into()),
            home: Some("rel/home".into()),
        };
        assert_eq!(resolve_state_dir(&i).unwrap(), None);
    }
}
