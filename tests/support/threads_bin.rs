//! The herdr-threads binary for real-threads tests: built from the pinned `third_party/herdr-threads` checkout
//! into `target/threads-bin` (cargo makes this a no-op when current), or `HG_TEST_THREADS_BINARY`; either way
//! its `--version` must report the submodule's Cargo.toml version, so a stale cached build fails loudly.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `[package] version` of the pinned checkout's Cargo.toml.
pub fn pinned_version() -> String {
    let path = crate_root().join("third_party/herdr-threads/Cargo.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {} (git submodule update --init?): {e}",
            path.display()
        )
    });
    let manifest: toml::Table = toml::from_str(&text).expect("threads Cargo.toml parses");
    manifest["package"]["version"]
        .as_str()
        .expect("package.version is a string")
        .to_owned()
}

/// `Ok` when `bin --version` prints exactly `herdr-threads <expected>`; otherwise a message naming the binary,
/// what it reported, what the pin expects and how to fix it.
pub fn check_version(bin: &Path, expected: &str) -> Result<(), String> {
    let out = Command::new(bin) // isolation-ok: --version only; reads no state
        .arg("--version")
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: cannot run --version: {e}", bin.display()))?;
    let reported = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.status.success() && reported == format!("herdr-threads {expected}") {
        Ok(())
    } else {
        Err(format!(
            "herdr-threads binary {} reports {reported:?}, but third_party/herdr-threads is pinned at {expected}; \
             rebuild it from the submodule or point HG_TEST_THREADS_BINARY at a {expected} build",
            bin.display()
        ))
    }
}

fn resolve() -> Result<PathBuf, String> {
    let bin = if let Some(path) = std::env::var_os("HG_TEST_THREADS_BINARY") {
        super::validated_binary(PathBuf::from(path))
    } else {
        if std::env::var("HG_HERDR_MATRIX").as_deref() == Ok("1") {
            return Err(
                "matrix requires a prebuilt HG_TEST_THREADS_BINARY; nested Cargo is forbidden"
                    .into(),
            );
        }
        let target = crate_root().join("target/threads-bin");
        let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())) // isolation-ok: cargo build of the pinned threads checkout
            .args(["build", "--locked", "--offline", "--manifest-path"])
            .arg(crate_root().join("third_party/herdr-threads/Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .stdin(Stdio::null())
            .status()
            .map_err(|e| format!("cargo runs: {e}"))?;
        if !status.success() {
            return Err("cargo build of third_party/herdr-threads failed".into());
        }
        let bin = target.join("debug/herdr-threads");
        if !bin.is_file() {
            return Err(format!("{} missing after the build", bin.display()));
        }
        bin
    };
    check_version(&bin, &pinned_version())?;
    Ok(bin)
}

/// The verified herdr-threads binary, resolved (built and version-checked) once per test process. Panics with
/// the reason on any failure: a test that needs threads must never run against the wrong daemon or skip.
pub fn threads_binary() -> PathBuf {
    static BIN: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BIN.get_or_init(resolve)
        .clone()
        .unwrap_or_else(|e| panic!("{e}"))
}
