//! Daemon composition root: registers kinds, commands and loops. Owned by hg-zmi.18 (this placeholder by hg-zmi.4).
use crate::daemon::{DaemonCtx, registry::Registry};

/// Registers every component. hg-zmi.18 replaces this body; until then the daemon serves only built-ins.
pub async fn compose(_reg: &mut Registry, _ctx: &DaemonCtx) -> anyhow::Result<()> {
    Ok(())
}
