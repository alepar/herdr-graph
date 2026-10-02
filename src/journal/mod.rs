//! SQLite op/effect journal and op state machine (spec §3.4). Owned by hg-zmi.3.
//!
//! Every state transition is a single `UPDATE ... WHERE op_id=? AND state IN (...)`; zero rows
//! changed means the transition was not legal. That compare-and-set is what makes cancel versus
//! apply linearizable.
use crate::model::change::{ChangeRequest, Requester};
use crate::model::effect::{EffectRecord, EffectStatus};
use crate::model::operation::{OpState, Rejection};
use crate::model::{ActionId, AnyId, CommitId, EffectId, OpId, Timestamp};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(test)]
mod tests;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ops(
  op_id TEXT PRIMARY KEY, seq INTEGER NOT NULL UNIQUE, kind TEXT NOT NULL, request_json TEXT NOT NULL,
  state TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, commit_oid TEXT, action_id TEXT,
  rejection_json TEXT, supersedes TEXT, superseded_by TEXT, admitted_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS ops_state_seq ON ops(state, seq);
CREATE TABLE IF NOT EXISTS effects(
  effect_id TEXT PRIMARY KEY, op_id TEXT NOT NULL, object_id TEXT NOT NULL, kind TEXT NOT NULL,
  object_rev INTEGER NOT NULL, status TEXT NOT NULL, record_json TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS effects_status ON effects(status);
CREATE INDEX IF NOT EXISTS effects_object ON effects(object_id);
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

pub struct Journal {
    conn: Mutex<Connection>,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpRow {
    pub op: OpId,
    pub seq: i64,
    pub request: ChangeRequest,
    pub state: OpState,
    pub attempts: u32,
    pub commit: Option<CommitId>,
    pub action: Option<ActionId>,
    pub rejection: Option<Rejection>,
    pub superseded_by: Option<OpId>,
    pub admitted_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CancelOutcome {
    Cancelled,
    NotCancellable(OpState),
    Unknown,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid transition for {op}: {from:?} → {to}")]
    Transition { op: String, from: OpState, to: &'static str },
}

type Result<T> = std::result::Result<T, JournalError>;

/// serde snake_case name of a unit-variant enum.
fn name_of<T: Serialize>(x: &T) -> Result<String> {
    Ok(serde_json::to_value(x)?.as_str().unwrap_or_default().to_owned())
}

fn ts(t: Timestamp) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn conv_err(col: usize, e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, Box::new(e))
}

fn parse_ts(col: usize, s: &str) -> rusqlite::Result<Timestamp> {
    chrono::DateTime::parse_from_rfc3339(s).map(|t| t.to_utc()).map_err(|e| conv_err(col, e))
}

fn parse_id<T: std::str::FromStr<Err = crate::model::IdError>>(col: usize, s: &str) -> rusqlite::Result<T> {
    s.parse().map_err(|e| conv_err(col, e))
}

fn parse_json<T: DeserializeOwned>(col: usize, s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|e| conv_err(col, e))
}

const OP_COLS: &str = "op_id, seq, request_json, state, attempts, commit_oid, action_id, rejection_json, \
                       superseded_by, admitted_at, updated_at";

fn op_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<OpRow> {
    let opt = |i: usize| r.get::<_, Option<String>>(i);
    Ok(OpRow {
        op: parse_id(0, &r.get::<_, String>(0)?)?,
        seq: r.get(1)?,
        request: parse_json(2, &r.get::<_, String>(2)?)?,
        state: parse_json(3, &format!("\"{}\"", r.get::<_, String>(3)?))?,
        attempts: r.get(4)?,
        commit: opt(5)?.map(CommitId),
        action: opt(6)?.map(|s| parse_id(6, &s)).transpose()?,
        rejection: opt(7)?.map(|s| parse_json(7, &s)).transpose()?,
        superseded_by: opt(8)?.map(|s| parse_id(8, &s)).transpose()?,
        admitted_at: parse_ts(9, &r.get::<_, String>(9)?)?,
        updated_at: parse_ts(10, &r.get::<_, String>(10)?)?,
    })
}

/// SQL list of quoted state names (they are fixed identifiers, never user input).
fn quoted_names<T: Serialize>(items: &[T]) -> Result<String> {
    let names: Result<Vec<String>> = items.iter().map(|s| name_of(s).map(|n| format!("'{n}'"))).collect();
    Ok(names?.join(","))
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "user_version", 1)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn), path: path.to_path_buf() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn path_in(instance_root: &Path) -> PathBuf {
        instance_root.join(".graph-local").join("journal.sqlite3")
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn admit(&self, req: &ChangeRequest, now: Timestamp) -> Result<OpId> {
        let op = OpId::new();
        self.admit_with_id(&op, req, now)?;
        Ok(op)
    }

    pub fn admit_with_id(&self, op: &OpId, req: &ChangeRequest, now: Timestamp) -> Result<()> {
        let json = serde_json::to_string(req)?;
        let kind = name_of(&req.kind)?;
        let t = ts(now);
        self.conn().execute(
            "INSERT INTO ops(op_id, seq, kind, request_json, state, attempts, supersedes, admitted_at, updated_at)
             VALUES(?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM ops), ?2, ?3, 'admitted', 0, ?4, ?5, ?5)",
            params![op.as_str(), kind, json, req.supersedes.as_ref().map(OpId::as_str), t],
        )?;
        Ok(())
    }

    fn get_on(conn: &Connection, op: &OpId) -> Result<Option<OpRow>> {
        Ok(conn
            .query_row(&format!("SELECT {OP_COLS} FROM ops WHERE op_id=?1"), [op.as_str()], op_row)
            .optional()?)
    }

    pub fn get(&self, op: &OpId) -> Result<Option<OpRow>> {
        Self::get_on(&self.conn(), op)
    }

    pub fn next_admitted(&self) -> Result<Option<OpRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {OP_COLS} FROM ops WHERE state='admitted' ORDER BY seq LIMIT 1"),
                [],
                op_row,
            )
            .optional()?)
    }

    /// The error for a transition that changed zero rows: reports the state the op is really in.
    fn bad_transition(conn: &Connection, op: &OpId, to: &'static str) -> JournalError {
        match Self::get_on(conn, op) {
            Ok(Some(row)) => JournalError::Transition { op: op.to_string(), from: row.state, to },
            Ok(None) => JournalError::Sql(rusqlite::Error::QueryReturnedNoRows),
            Err(e) => e,
        }
    }

    /// CAS admitted→applying, attempts += 1; Some(new attempts) or None if no longer admitted.
    pub fn begin_applying(&self, op: &OpId, now: Timestamp) -> Result<Option<u32>> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE ops SET state='applying', attempts=attempts+1, updated_at=?2 WHERE op_id=?1 AND state='admitted'",
            params![op.as_str(), ts(now)],
        )?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(conn.query_row("SELECT attempts FROM ops WHERE op_id=?1", [op.as_str()], |r| r.get(0))?))
    }

    /// From applying|admitted (the latter is recovery finding a trailer for a requeued op).
    pub fn finish_committed(
        &self,
        op: &OpId,
        commit: &CommitId,
        action: Option<&ActionId>,
        now: Timestamp,
    ) -> Result<()> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE ops SET state='committed', commit_oid=?2, action_id=?3, updated_at=?4
             WHERE op_id=?1 AND state IN ('applying','admitted')",
            params![op.as_str(), commit.0, action.map(ActionId::as_str), ts(now)],
        )?;
        if n == 0 {
            return Err(Self::bad_transition(&conn, op, "committed"));
        }
        Ok(())
    }

    /// applying|admitted → committed and, when `supersedes` is set, that op → superseded, in ONE transaction.
    /// A supersede target in a state that cannot be superseded is logged and skipped; any SQL error rolls back both.
    pub fn finish_committed_superseding(
        &self,
        op: &OpId,
        commit: &CommitId,
        action: Option<&ActionId>,
        supersedes: Option<&OpId>,
        now: Timestamp,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let n = tx.execute(
            "UPDATE ops SET state='committed', commit_oid=?2, action_id=?3, updated_at=?4
             WHERE op_id=?1 AND state IN ('applying','admitted')",
            params![op.as_str(), commit.0, action.map(ActionId::as_str), ts(now)],
        )?;
        if n == 0 {
            return Err(Self::bad_transition(&tx, op, "committed"));
        }
        if let Some(old) = supersedes {
            let n = tx.execute(
                "UPDATE ops SET state='superseded', superseded_by=?2, updated_at=?3
                 WHERE op_id=?1 AND state IN ('admitted','committed','rejected','failed')",
                params![old.as_str(), op.as_str(), ts(now)],
            )?;
            if n == 0 {
                eprintln!("herdr-graph: could not mark {old} superseded by {op}: target is not in a supersedable state");
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Test-only SQL hook for failure injection (triggers).
    #[cfg(test)]
    pub(crate) fn execute_batch_for_test(&self, sql: &str) -> Result<()> {
        self.conn().execute_batch(sql)?;
        Ok(())
    }

    pub fn finish_rejected(&self, op: &OpId, r: &Rejection, now: Timestamp) -> Result<()> {
        let json = serde_json::to_string(r)?;
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE ops SET state='rejected', rejection_json=?2, updated_at=?3 WHERE op_id=?1 AND state='applying'",
            params![op.as_str(), json, ts(now)],
        )?;
        if n == 0 {
            return Err(Self::bad_transition(&conn, op, "rejected"));
        }
        Ok(())
    }

    pub fn finish_failed(&self, op: &OpId, reason: &str, now: Timestamp) -> Result<()> {
        let r = Rejection { reason: reason.to_owned(), explanation: String::new(), current_revs: vec![] };
        let json = serde_json::to_string(&r)?;
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE ops SET state='failed', rejection_json=?2, updated_at=?3 WHERE op_id=?1 AND state='applying'",
            params![op.as_str(), json, ts(now)],
        )?;
        if n == 0 {
            return Err(Self::bad_transition(&conn, op, "failed"));
        }
        Ok(())
    }

    /// applying→admitted. `keep_attempt=false` decrements attempts (infra error: not a poison attempt).
    pub fn requeue(&self, op: &OpId, keep_attempt: bool, now: Timestamp) -> Result<()> {
        let conn = self.conn();
        let dec = i64::from(!keep_attempt);
        let n = conn.execute(
            "UPDATE ops SET state='admitted', attempts=MAX(attempts-?2, 0), updated_at=?3
             WHERE op_id=?1 AND state='applying'",
            params![op.as_str(), dec, ts(now)],
        )?;
        if n == 0 {
            return Err(Self::bad_transition(&conn, op, "admitted"));
        }
        Ok(())
    }

    /// CAS admitted|failed → cancelled (spec §3.7). Anything else → NotCancellable(state).
    pub fn cancel(&self, op: &OpId, now: Timestamp) -> Result<CancelOutcome> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE ops SET state='cancelled', updated_at=?2 WHERE op_id=?1 AND state IN ('admitted','failed')",
            params![op.as_str(), ts(now)],
        )?;
        if n == 1 {
            return Ok(CancelOutcome::Cancelled);
        }
        Ok(match Self::get_on(&conn, op)? {
            Some(row) => CancelOutcome::NotCancellable(row.state),
            None => CancelOutcome::Unknown,
        })
    }

    /// old ∈ {admitted, committed, rejected, failed} → superseded, superseded_by = by.
    pub fn supersede(&self, old: &OpId, by: &OpId, now: Timestamp) -> Result<()> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE ops SET state='superseded', superseded_by=?2, updated_at=?3
             WHERE op_id=?1 AND state IN ('admitted','committed','rejected','failed')",
            params![old.as_str(), by.as_str(), ts(now)],
        )?;
        if n == 0 {
            return Err(Self::bad_transition(&conn, old, "superseded"));
        }
        Ok(())
    }

    pub fn set_requester(&self, op: &OpId, requester: &Requester, now: Timestamp) -> Result<()> {
        let conn = self.conn();
        let row = Self::get_on(&conn, op)?.ok_or(JournalError::Sql(rusqlite::Error::QueryReturnedNoRows))?;
        let mut req = row.request;
        req.requester = requester.clone();
        conn.execute(
            "UPDATE ops SET request_json=?2, updated_at=?3 WHERE op_id=?1",
            params![op.as_str(), serde_json::to_string(&req)?, ts(now)],
        )?;
        Ok(())
    }

    /// Newest first; empty `states` = all.
    pub fn list(&self, states: &[OpState], limit: usize) -> Result<Vec<OpRow>> {
        let filter =
            if states.is_empty() { String::new() } else { format!("WHERE state IN ({})", quoted_names(states)?) };
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!("SELECT {OP_COLS} FROM ops {filter} ORDER BY seq DESC LIMIT ?1"))?;
        let rows = stmt.query_map([limit as i64], op_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn counts(&self) -> Result<BTreeMap<String, u64>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT state, COUNT(*) FROM ops GROUP BY state")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
        Ok(rows)
    }

    pub fn checkpoint(&self) -> Result<Option<CommitId>> {
        Ok(self.meta_get("checkpoint_commit")?.map(CommitId))
    }

    pub fn set_checkpoint(&self, c: &CommitId) -> Result<()> {
        self.meta_set("checkpoint_commit", &c.0)
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn().query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn meta_delete(&self, key: &str) -> Result<()> {
        self.conn().execute("DELETE FROM meta WHERE key=?1", [key])?;
        Ok(())
    }

    // Effects table: schema owned here, rows written by the reconciler (hg-zmi.7).

    fn upsert_effect_on(conn: &Connection, e: &EffectRecord) -> Result<()> {
        conn.execute(
            "INSERT INTO effects(effect_id, op_id, object_id, kind, object_rev, status, record_json, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(effect_id) DO UPDATE SET status=excluded.status, record_json=excluded.record_json,
               object_rev=excluded.object_rev, updated_at=excluded.updated_at",
            params![
                e.id.as_str(),
                e.op.as_str(),
                e.object.as_str(),
                e.kind.as_str(),
                e.object_rev as i64,
                name_of(&e.status)?,
                serde_json::to_string(e)?,
                ts(e.updated_at)
            ],
        )?;
        Ok(())
    }

    pub fn upsert_effect(&self, e: &EffectRecord) -> Result<()> {
        Self::upsert_effect_on(&self.conn(), e)
    }

    fn get_effect_on(conn: &Connection, id: &EffectId) -> Result<Option<EffectRecord>> {
        let json: Option<String> = conn
            .query_row("SELECT record_json FROM effects WHERE effect_id=?1", [id.as_str()], |r| r.get(0))
            .optional()?;
        Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
    }

    pub fn get_effect(&self, id: &EffectId) -> Result<Option<EffectRecord>> {
        Self::get_effect_on(&self.conn(), id)
    }

    fn effects_where(&self, clause: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<EffectRecord>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare(&format!("SELECT record_json FROM effects WHERE {clause} ORDER BY updated_at, effect_id"))?;
        let jsons = stmt.query_map(args, |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        jsons.iter().map(|j| Ok(serde_json::from_str(j)?)).collect()
    }

    pub fn effects_with_status(&self, s: &[EffectStatus]) -> Result<Vec<EffectRecord>> {
        if s.is_empty() {
            return Ok(Vec::new());
        }
        self.effects_where(&format!("status IN ({})", quoted_names(s)?), &[])
    }

    pub fn effects_for_object(&self, o: &AnyId) -> Result<Vec<EffectRecord>> {
        self.effects_where("object_id=?1", &[&o.as_str()])
    }

    pub fn set_effect_status(
        &self,
        id: &EffectId,
        s: EffectStatus,
        last_error: Option<&str>,
        now: Timestamp,
    ) -> Result<()> {
        let conn = self.conn();
        let Some(mut e) = Self::get_effect_on(&conn, id)? else {
            return Err(JournalError::Sql(rusqlite::Error::QueryReturnedNoRows));
        };
        e.status = s;
        e.last_error = last_error.map(str::to_owned);
        e.updated_at = now;
        Self::upsert_effect_on(&conn, &e)
    }
}
