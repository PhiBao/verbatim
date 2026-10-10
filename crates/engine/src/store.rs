//! Durable state.
//!
//! An **append-only event log is the only source of truth**; runs, steps,
//! sources, claims and artifacts are projections of it. This is the pattern
//! that makes `kill -9` mid-run and resume correct: a hard process loss before
//! an effect has settled leaves no record of it, so the resumed run redoes the
//! work rather than guessing, and a redo is a no-op at the tool boundary
//! because every effect carries an idempotency key.
//!
//! It also means "model-visible means logged" holds in the direction we care
//! about: **user-visible means logged**. Every claim on screen can be traced
//! back through the log to the event that produced it.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

/// A fact appended to the log. Everything else is derived from these.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Event {
    pub seq: u64,
    pub run_id: Uuid,
    pub at: DateTime<Utc>,
    pub kind: String,
    pub body: serde_json::Value,
}

#[derive(Clone)]
pub struct Store {
    conn: std::sync::Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path).context("open sqlite")?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = FULL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS events (
                seq      INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id   TEXT    NOT NULL,
                at       TEXT    NOT NULL,
                kind     TEXT    NOT NULL,
                body     TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS events_run ON events(run_id, seq);

            -- Idempotency ledger: exactly-once tool effects.
            CREATE TABLE IF NOT EXISTS effects (
                idempotency_key TEXT PRIMARY KEY,
                run_id          TEXT NOT NULL,
                tool            TEXT NOT NULL,
                args_hash       TEXT NOT NULL,
                result_ref      TEXT,
                at              TEXT NOT NULL
            );

            -- Correlates a run with the neatlogs trace id we got back for it.
            CREATE TABLE IF NOT EXISTS traces (
                run_id     TEXT NOT NULL,
                trace_id   TEXT NOT NULL,
                session    TEXT NOT NULL,
                at         TEXT NOT NULL,
                PRIMARY KEY (run_id, trace_id)
            );
            "#,
        )
        .context("migrate")?;
        Ok(Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("open memory db")?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL, at TEXT NOT NULL,
                kind TEXT NOT NULL, body TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS events_run ON events(run_id, seq);
            CREATE TABLE IF NOT EXISTS effects (
                idempotency_key TEXT PRIMARY KEY,
                run_id TEXT NOT NULL, tool TEXT NOT NULL,
                args_hash TEXT NOT NULL, result_ref TEXT, at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS traces (
                run_id TEXT NOT NULL, trace_id TEXT NOT NULL,
                session TEXT NOT NULL, at TEXT NOT NULL,
                PRIMARY KEY (run_id, trace_id)
            );
            "#,
        )
        .context("migrate memory")?;
        Ok(Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    /// Appends one event.
    pub fn append(&self, run_id: Uuid, kind: &str, body: serde_json::Value) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (run_id, at, kind, body) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                run_id.to_string(),
                Utc::now().to_rfc3339(),
                kind,
                body.to_string()
            ],
        )
        .context("append event")?;
        Ok(conn.last_insert_rowid() as u64)
    }

    /// Every event for a run, in order.
    pub fn events(&self, run_id: Uuid) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT seq, run_id, at, kind, body FROM events WHERE run_id = ?1 ORDER BY seq")
            .context("prepare events")?;
        let rows = stmt
            .query_map([run_id.to_string()], |r| {
                let at: String = r.get(2)?;
                Ok(Event {
                    seq: r.get(0)?,
                    run_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap_or_default(),
                    at: DateTime::parse_from_rfc3339(&at)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    kind: r.get(3)?,
                    body: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or(json_null()),
                })
            })
            .context("query events")?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Records a settled effect. Returns `true` if this call settled it and
    /// `false` if a previous run already had — which is how the caller knows to
    /// reuse the stored result instead of repeating the side effect.
    pub fn settle_effect(
        &self,
        run_id: Uuid,
        key: &str,
        tool: &str,
        args_hash: &str,
        result_ref: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let inserted = conn
            .execute(
                "INSERT OR IGNORE INTO effects (idempotency_key, run_id, tool, args_hash, result_ref, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    key,
                    run_id.to_string(),
                    tool,
                    args_hash,
                    result_ref,
                    Utc::now().to_rfc3339()
                ],
            )
            .context("settle effect")?;
        Ok(inserted > 0)
    }

    pub fn effect_result_ref(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let found = conn
            .query_row(
                "SELECT result_ref FROM effects WHERE idempotency_key = ?1",
                [key],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .context("lookup effect")?;
        Ok(found.flatten())
    }

    /// Every settled effect for a run. The proof that a resumed run repeated
    /// nothing.
    pub fn effects(&self, run_id: Uuid) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT idempotency_key, tool, args_hash, result_ref, at FROM effects
                 WHERE run_id = ?1 ORDER BY at",
            )
            .context("prepare effects")?;
        let rows = stmt
            .query_map([run_id.to_string()], |r| {
                Ok(serde_json::json!({
                    "key": r.get::<_, String>(0)?,
                    "tool": r.get::<_, String>(1)?,
                    "args_hash": r.get::<_, String>(2)?,
                    "result_ref": r.get::<_, Option<String>>(3)?,
                    "at": r.get::<_, String>(4)?,
                }))
            })
            .context("query effects")?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn record_trace(&self, run_id: Uuid, trace_id: &str, session: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO traces (run_id, trace_id, session, at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![run_id.to_string(), trace_id, session, Utc::now().to_rfc3339()],
        )
        .context("record trace")?;
        Ok(())
    }

    pub fn trace_ids(&self, run_id: Uuid) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT trace_id FROM traces WHERE run_id = ?1 ORDER BY at")
            .context("prepare traces")?;
        let rows = stmt
            .query_map([run_id.to_string()], |r| r.get::<_, String>(0))
            .context("query traces")?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Total events, for the health endpoint.
    pub fn event_count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .optional()
            .map(|o| o.unwrap_or(0) as u64)
            .context("count events")
    }
}

fn json_null() -> serde_json::Value {
    serde_json::Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_append_only_and_ordered() {
        let s = Store::in_memory().unwrap();
        let run = Uuid::new_v4();
        s.append(run, "run.started", serde_json::json!({"q": "hi"})).unwrap();
        s.append(run, "claim.drafted", serde_json::json!({"t": "x"})).unwrap();
        let evs = s.events(run).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].kind, "run.started");
        assert!(evs[1].seq > evs[0].seq);
    }

    #[test]
    fn effects_settle_exactly_once() {
        let s = Store::in_memory().unwrap();
        let run = Uuid::new_v4();
        assert!(s.settle_effect(run, "k1", "fetch", "h", Some("r1")).unwrap());
        // A duplicate attempt must report that it was already settled.
        assert!(!s.settle_effect(run, "k1", "fetch", "h", Some("r2")).unwrap());
        assert_eq!(s.effect_result_ref("k1").unwrap().as_deref(), Some("r1"));
        assert_eq!(s.effects(run).unwrap().len(), 1);
    }

    #[test]
    fn events_are_scoped_per_run() {
        let s = Store::in_memory().unwrap();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        s.append(a, "x", serde_json::json!({})).unwrap();
        s.append(b, "y", serde_json::json!({})).unwrap();
        assert_eq!(s.events(a).unwrap().len(), 1);
        assert_eq!(s.events(b).unwrap()[0].kind, "y");
    }
}
