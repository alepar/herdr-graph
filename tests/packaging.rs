//! Packaging checks: plugin manifest, build script, README structure (spec §12).

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn argv(v: &toml::Value) -> Vec<String> {
    v["command"]
        .as_array()
        .expect("command is an array")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn manifest_parses_with_expected_values() {
    let text = std::fs::read_to_string(root().join("herdr-plugin.toml")).unwrap();
    let m: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(m["id"].as_str(), Some("herdr-graph"));
    assert_eq!(m["min_herdr_version"].as_str(), Some("0.9.1"));
    assert_eq!(m["platforms"].as_array().unwrap(), &[toml::Value::String("macos".into())]);

    let build = &m["build"].as_array().unwrap()[0];
    assert_eq!(argv(build), ["./scripts/build.sh"]);

    let startup = &m["startup"].as_array().unwrap()[0];
    assert_eq!(argv(startup), ["./bin/herdr-graph", "daemon", "--ensure"]);

    let actions = m["actions"].as_array().unwrap();
    let find = |id: &str| actions.iter().find(|a| a["id"].as_str() == Some(id)).unwrap_or_else(|| panic!("action {id}"));
    assert_eq!(argv(find("status")), ["./bin/herdr-graph", "status"]);
    assert_eq!(argv(find("doctor")), ["./bin/herdr-graph", "doctor"]);
    assert_eq!(actions.len(), 2);
}

#[test]
fn build_script_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(root().join("scripts/build.sh")).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111, "scripts/build.sh must be executable, mode {mode:o}");
}

#[test]
#[ignore = "runs a release build; run with: cargo test --test packaging -- --ignored build_script_places_binary"]
fn build_script_places_binary() {
    let bin_dir = root().join("bin");
    let out = Command::new(root().join("scripts/build.sh")).output().unwrap();
    assert!(out.status.success(), "build.sh failed: {}", String::from_utf8_lossy(&out.stderr));
    let bin = bin_dir.join("herdr-graph");
    let ver = Command::new(&bin).arg("--version").output();
    let _ = std::fs::remove_dir_all(&bin_dir);
    let ver = ver.unwrap();
    assert!(ver.status.success(), "--version failed: {}", String::from_utf8_lossy(&ver.stderr));
}

/// Links the plugin into a PRIVATE, isolated Herdr configuration (own XDG dirs and socket
/// under a temp root, `--disabled` so no build/startup runs). Never touches the live session.
#[cfg(feature = "private-herdr")]
#[test]
fn plugin_links_into_private_herdr() {
    // Short root: macOS sockaddr_un path limit.
    let private = tempfile::Builder::new().prefix("hg").tempdir_in("/tmp").unwrap();
    let p = private.path();
    for d in ["config", "state", "runtime"] {
        std::fs::create_dir_all(p.join(d)).unwrap();
    }
    let socket = p.join("s.sock");
    let herdr = |args: &[&str]| {
        Command::new("herdr")
            .args(args)
            .env("XDG_CONFIG_HOME", p.join("config"))
            .env("XDG_STATE_HOME", p.join("state"))
            .env("XDG_RUNTIME_DIR", p.join("runtime"))
            .env("HERDR_CONFIG_PATH", p.join("absent-config.toml"))
            .env("HERDR_SOCKET_PATH", &socket)
            .env("HERDR_PLUGIN_STATE_DIR", p.join("state"))
            .env_remove("HERDR_ENV")
            .env_remove("HERDR_CLIENT_SOCKET_PATH")
            .env_remove("HERDR_SESSION")
            .env_remove("HERDR_PANE_ID")
            .output()
            .unwrap()
    };
    let link = herdr(&["plugin", "link", root().to_str().unwrap(), "--disabled"]);
    assert!(link.status.success(), "link failed: {}", String::from_utf8_lossy(&link.stderr));
    let list = herdr(&["plugin", "list"]);
    assert!(list.status.success(), "list failed: {}", String::from_utf8_lossy(&list.stderr));
    let text = String::from_utf8_lossy(&list.stdout);
    assert!(text.contains("herdr-graph"), "plugin list lacks herdr-graph: {text}");
    assert!(!socket.exists(), "link must not start a server");
}

#[test]
fn readme_has_required_sections() {
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    let headings: Vec<&str> = readme.lines().filter(|l| l.starts_with("## ")).collect();
    for want in [
        "## Install",
        "## Create an instance",
        "## Claude setup",
        "## /seat",
        "## Plan, confirm, relay",
        "## Undo",
        "## Ops",
        "## Threads integration and amendment status",
        "## Test tiers",
        "## Verification matrix",
        "## Design documents",
        "## License",
    ] {
        assert!(headings.contains(&want), "README missing heading {want}; has {headings:?}");
    }
    assert!(readme.contains("ht-5nb"));
    assert!(readme.contains("ln -sfn"));
}
