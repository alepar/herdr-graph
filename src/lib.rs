//! herdr-graph: Git-backed organization graph for Herdr (teamspaces, seats, clones).
//! See docs/superpowers/runs/2026-10-02-herdr-graph-implementation/2026-10-02-herdr-graph-mvp-design.md.

pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod failpoint;
pub mod herdr;
pub mod ipc;
pub mod journal;
pub mod model;
pub mod observe;
pub mod plan;
pub mod ports;
pub mod reconcile;
pub mod store;
pub mod templates;
pub mod threads;
pub mod transcripts;
pub mod undo;
pub mod writer;
