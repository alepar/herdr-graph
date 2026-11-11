//! `sessions` (spec §2.5): every session of the scoped clones, with its transcript. Daemon-free, read-only.
use crate::daemon::client::{EXIT_NO_INSTANCE, instance_or_none};
use crate::inspect::inventory::Inventory;
use crate::inspect::output::{Envelope, Format, print};
use crate::inspect::scope::{AllowClone, Scope, ScopeArg};
use crate::inspect::sessions::{SessionsQuery, build};
use crate::inspect::{fail, pin_head};
use crate::model::Timestamp;
use crate::ports::store::Store;
use clap::Subcommand;
use std::process::ExitCode;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Sessions of the scoped clones, newest first, with transcript paths (daemon-free).
    Sessions {
        /// Scope: global | teamspace <t> | seat <s> | clone <c>; omitted = whole instance.
        #[arg(num_args = 0..=2)]
        scope: Vec<String>,
        /// Only sessions started at or after this time: RFC 3339, YYYY-MM-DD, or a duration like 2d.
        #[arg(long, value_parser = crate::inspect::parse_since)]
        since: Option<Timestamp>,
        /// Maximum rows (default 50).
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Print one JSON document (schema 1) on stdout; notices go to stderr.
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Commands) -> anyhow::Result<ExitCode> {
    let Some(root) = instance_or_none() else {
        return Ok(ExitCode::from(EXIT_NO_INSTANCE));
    };
    let Commands::Sessions {
        scope,
        since,
        limit,
        json,
    } = cmd;
    let result = crate::store::GitStore::open(&root)
        .map_err(anyhow::Error::from)
        .and_then(|store| {
            let (env, format) = prepare(&store, &scope, since, limit, json)?;
            print(&env, format, &[])?;
            Ok(())
        });
    Ok(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    })
}

type Prepared = (Envelope<crate::inspect::sessions::SessionsView>, Format);

/// HEAD is pinned once here and handed to Scope, Inventory and the envelope.
fn prepare(
    store: &dyn Store,
    scope: &[String],
    since: Option<Timestamp>,
    limit: usize,
    json: bool,
) -> anyhow::Result<Prepared> {
    let head = pin_head(store)?;
    let arg = ScopeArg::parse(scope, AllowClone::Yes)?;
    let scope = Scope::resolve(&arg, store, &head)?;
    let inv = Inventory::load(store, &head, &scope)?;
    let view = build(
        &inv,
        &SessionsQuery {
            scope,
            since,
            limit,
        },
    );
    Ok((Envelope::new(&head, view), Format::from_flag(json)))
}

/// `run` against an open store and explicit streams, for in-process tests.
#[cfg(test)]
fn execute(
    store: &dyn Store,
    scope: &[String],
    since: Option<Timestamp>,
    limit: usize,
    json: bool,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> anyhow::Result<()> {
    let (env, format) = prepare(store, scope, since, limit, json)?;
    crate::inspect::output::emit(&env, format, &[], out, err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::fixture::{Fx, session};
    use crate::model::harness::Harness;
    use crate::model::{CloneLifecycle, Lifecycle};

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn sessions_is_read_only() {
        let mut fx = Fx::new();
        let t = fx.teamspace("proj", Lifecycle::Active);
        let st = fx.seat(&t, "foreman", Lifecycle::Active);
        fx.tombstone(&st, "purged");
        fx.clone_rec(
            &st,
            "main",
            CloneLifecycle::Active,
            vec![session(
                Harness::Claude,
                "n1",
                "2026-11-01T00:00:00Z",
                Some("/t.jsonl"),
                None,
            )],
            None,
        );
        let head = fx.commit("c");
        for scope in ["", "global", "teamspace proj", "seat foreman", "clone main"] {
            for json in [false, true] {
                let (mut out, mut err) = (Vec::new(), Vec::new());
                execute(&fx.store, &words(scope), None, 50, json, &mut out, &mut err).unwrap();
                assert_eq!(fx.store.head().unwrap(), head, "{scope} json={json}");
                assert!(err.is_empty());
                if json {
                    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
                    let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
                    for k in ["schema", "head", "tombstones", "total", "rows"] {
                        assert!(keys.iter().any(|x| x == k), "{scope}: {keys:?}");
                    }
                    assert_eq!(keys.len(), 5, "{keys:?}");
                    assert_eq!(v["head"], head.0);
                    assert_eq!(v["schema"], 1);
                    assert_eq!(
                        v["rows"].as_array().unwrap().len(),
                        usize::from(scope != "global")
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_clone_is_an_error() {
        let mut fx = Fx::new();
        fx.commit("c");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let e = execute(
            &fx.store,
            &words("clone nosuch"),
            None,
            50,
            true,
            &mut out,
            &mut err,
        )
        .unwrap_err();
        assert!(format!("{e:#}").contains("nosuch"), "{e:#}");
        assert!(out.is_empty());
    }
}
