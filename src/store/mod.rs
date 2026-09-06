//! Transactional run storage on SQLite.
//!
//! One local SQLite file is the authoritative ledger for graphs, runs,
//! attempts, receipts, gate packets, gate decisions, and the ordered event
//! log that ties them together. Application records are append-only: a
//! graph revision is bound to its content digest and can never be replaced,
//! an attempt row is written once when it is sealed, and acceptance exists
//! only as a `receipt_admitted` event written in the same transaction as the
//! receipt row (a trigger refuses the event without the row). Derived views
//! are computed from these tables and may be rebuilt at any time.
//!
//! The store reuses SQLite's atomic commit for crash recovery: WAL journal,
//! `synchronous=FULL`, and foreign keys on. It is a trusted local ledger,
//! not a tamper-proof one.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::Value;

use crate::contracts::{
    Attempt, GateDecision, GatePacket, GraphRef, GraphRun, GraphSpec, NodeId, RunId,
    VerifierReceipt, parse_record,
};
use crate::digests::{CanonicalError, graph_spec_digest};

/// The store schema version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Longest artifact locator the store accepts.
pub const MAX_LOCATOR_LEN: usize = 2048;

/// Longest JSON record the store accepts, in bytes.
pub const MAX_RECORD_LEN: usize = 4 * 1024 * 1024;

const SCHEMA_V1: &str = "
CREATE TABLE graphs (
    graph_id    TEXT    NOT NULL,
    revision    INTEGER NOT NULL,
    spec_digest TEXT    NOT NULL,
    spec_json   TEXT    NOT NULL,
    stored_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (graph_id, revision)
) STRICT;

CREATE TABLE runs (
    run_id             TEXT    NOT NULL PRIMARY KEY,
    graph_id           TEXT    NOT NULL,
    revision           INTEGER NOT NULL,
    budget_lineage_ref TEXT,
    run_json           TEXT    NOT NULL,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (graph_id, revision) REFERENCES graphs (graph_id, revision)
) STRICT;

CREATE TABLE events (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       TEXT    NOT NULL REFERENCES runs (run_id),
    node_id      TEXT,
    kind         TEXT    NOT NULL,
    payload_json TEXT    NOT NULL,
    recorded_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX events_by_run ON events (run_id, seq);

CREATE TABLE attempts (
    run_id       TEXT    NOT NULL REFERENCES runs (run_id),
    node_id      TEXT    NOT NULL,
    number       INTEGER NOT NULL,
    attempt_json TEXT    NOT NULL,
    recorded_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (run_id, node_id, number)
) STRICT;

CREATE TABLE receipts (
    run_id       TEXT    NOT NULL,
    receipt_id   TEXT    NOT NULL,
    node_id      TEXT    NOT NULL,
    number       INTEGER NOT NULL,
    verdict      TEXT    NOT NULL,
    receipt_json TEXT    NOT NULL,
    recorded_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (run_id, receipt_id),
    FOREIGN KEY (run_id, node_id, number) REFERENCES attempts (run_id, node_id, number)
) STRICT;

CREATE TABLE gate_packets (
    run_id      TEXT NOT NULL REFERENCES runs (run_id),
    packet_id   TEXT NOT NULL,
    node_id     TEXT NOT NULL,
    packet_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (run_id, packet_id)
) STRICT;

CREATE TABLE gate_decisions (
    run_id        TEXT NOT NULL,
    packet_id     TEXT NOT NULL,
    decision      TEXT NOT NULL,
    decision_json TEXT NOT NULL,
    recorded_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (run_id, packet_id),
    FOREIGN KEY (run_id, packet_id) REFERENCES gate_packets (run_id, packet_id)
) STRICT;

CREATE TABLE artifacts (
    run_id      TEXT    NOT NULL REFERENCES runs (run_id),
    digest      TEXT    NOT NULL,
    kind        TEXT    NOT NULL,
    locator     TEXT    NOT NULL CHECK (length(locator) BETWEEN 1 AND 2048),
    size_bytes  INTEGER NOT NULL CHECK (size_bytes >= 0),
    recorded_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (run_id, digest)
) STRICT;

CREATE TRIGGER receipt_admitted_requires_receipt
BEFORE INSERT ON events
WHEN NEW.kind = 'receipt_admitted'
 AND NOT EXISTS (
    SELECT 1 FROM receipts
     WHERE run_id = NEW.run_id
       AND receipt_id = json_extract(NEW.payload_json, '$.receipt_id'))
BEGIN
    SELECT RAISE(ABORT, 'receipt_admitted event without its receipt row');
END;

CREATE TRIGGER graphs_are_immutable
BEFORE UPDATE ON graphs
BEGIN
    SELECT RAISE(ABORT, 'graph revisions are immutable');
END;

CREATE TRIGGER attempts_are_immutable
BEFORE UPDATE ON attempts
BEGIN
    SELECT RAISE(ABORT, 'sealed attempts are immutable');
END;

CREATE TRIGGER receipts_are_immutable
BEFORE UPDATE ON receipts
BEGIN
    SELECT RAISE(ABORT, 'receipts are immutable');
END;

CREATE TRIGGER events_are_immutable
BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'events are immutable');
END;

CREATE TRIGGER events_are_not_deleted
BEFORE DELETE ON events
BEGIN
    SELECT RAISE(ABORT, 'events are never deleted');
END;
";

/// Why a store operation failed.
#[derive(Debug)]
pub enum StoreError {
    /// The file carries a schema this build does not support.
    UnsupportedSchema {
        /// The version found in the file.
        found: u32,
        /// The version this build supports.
        supported: u32,
    },
    /// The same graph revision already exists with different content.
    RevisionConflict {
        /// The revision.
        graph_ref: GraphRef,
        /// The digest already stored.
        stored: String,
        /// The digest of the document offered.
        offered: String,
    },
    /// A record refers to a graph, run, attempt, or packet that is not stored.
    MissingReference(String),
    /// A record with this identity is already stored.
    Duplicate(String),
    /// A record exceeds a size bound.
    TooLarge {
        /// What was too large.
        what: &'static str,
        /// Its size in bytes.
        size: usize,
        /// The bound.
        limit: usize,
    },
    /// The record could not be canonicalized or serialized.
    Canonical(CanonicalError),
    /// A stored record could not be read back as its type.
    Corrupt(String),
    /// SQLite reported an error.
    Sqlite(rusqlite::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::UnsupportedSchema { found, supported } => write!(
                f,
                "store schema version {found} is not supported; this build supports {supported}"
            ),
            StoreError::RevisionConflict {
                graph_ref,
                stored,
                offered,
            } => write!(
                f,
                "graph {}@{} is already stored with digest {stored}; refusing different content {offered}",
                graph_ref.graph_id, graph_ref.revision
            ),
            StoreError::MissingReference(what) => write!(f, "{what} is not stored"),
            StoreError::Duplicate(what) => write!(f, "{what} is already stored"),
            StoreError::TooLarge { what, size, limit } => {
                write!(f, "{what} is {size} bytes; the limit is {limit}")
            }
            StoreError::Canonical(e) => write!(f, "{e}"),
            StoreError::Corrupt(what) => write!(f, "stored record cannot be read: {what}"),
            StoreError::Sqlite(e) => write!(f, "sqlite: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

impl From<CanonicalError> for StoreError {
    fn from(e: CanonicalError) -> Self {
        StoreError::Canonical(e)
    }
}

/// Outcome of storing a graph revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredGraph {
    /// The revision.
    pub graph_ref: GraphRef,
    /// Its content digest.
    pub digest: String,
    /// Whether this call inserted it (false: an identical revision existed).
    pub inserted: bool,
}

/// One row of the event log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    /// Position in the log; strictly increasing, never reused.
    pub seq: u64,
    /// The run.
    pub run_id: RunId,
    /// The node, for node-level events.
    pub node_id: Option<NodeId>,
    /// Event kind.
    pub kind: String,
    /// Event payload.
    pub payload: Value,
    /// When the store recorded it.
    pub recorded_at: String,
}

/// Bounded reference to an artifact kept outside the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    /// Digest of the artifact bytes.
    pub digest: String,
    /// What kind of artifact it is.
    pub kind: String,
    /// Where it is kept.
    pub locator: String,
    /// Its size in bytes.
    pub size_bytes: u64,
}

/// Per-node counts derived from the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSummary {
    /// The node.
    pub node_id: NodeId,
    /// Sealed attempts.
    pub attempts: u64,
    /// Admitted receipts.
    pub receipts: u64,
    /// Verdict of the most recently admitted receipt.
    pub last_verdict: Option<String>,
}

/// A transaction over the store. Every write goes through one.
pub struct Tx<'a> {
    inner: rusqlite::Transaction<'a>,
}

/// The local ledger.
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

fn json_len_checked(what: &'static str, text: &str) -> Result<(), StoreError> {
    if text.len() > MAX_RECORD_LEN {
        return Err(StoreError::TooLarge {
            what,
            size: text.len(),
            limit: MAX_RECORD_LEN,
        });
    }
    Ok(())
}

fn is_constraint(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

impl Store {
    /// Open the store at `path`, creating and initializing it if absent.
    /// A file with a newer schema is refused without modification.
    pub fn open(path: impl AsRef<Path>) -> Result<Store, StoreError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let found: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        match found {
            0 => {
                conn.pragma_update(None, "journal_mode", "WAL")?;
                conn.pragma_update(None, "synchronous", "FULL")?;
                let tx = conn.unchecked_transaction()?;
                tx.execute_batch(SCHEMA_V1)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
                tx.commit()?;
            }
            SCHEMA_VERSION => {
                conn.pragma_update(None, "synchronous", "FULL")?;
            }
            other => {
                return Err(StoreError::UnsupportedSchema {
                    found: other,
                    supported: SCHEMA_VERSION,
                });
            }
        }
        Ok(Store { conn, path })
    }

    /// The file backing this store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The schema version of the open file.
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// The journal mode of the open file.
    pub fn journal_mode(&self) -> Result<String, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?)
    }

    /// Run `f` inside one write transaction. It commits only if `f` returns
    /// `Ok`; any error rolls back every write `f` made.
    pub fn transaction<T>(
        &mut self,
        f: impl FnOnce(&Tx<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let inner = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tx = Tx { inner };
        let value = f(&tx)?;
        tx.inner.commit()?;
        Ok(value)
    }

    /// Store a graph revision, binding it to its content digest. Storing an
    /// identical revision again is a no-op; different content at the same
    /// revision is refused.
    pub fn store_graph(&mut self, spec: &GraphSpec) -> Result<StoredGraph, StoreError> {
        let digest = graph_spec_digest(spec)?.as_str().to_string();
        let json = serde_json::to_string(spec).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        json_len_checked("graph spec", &json)?;
        let graph_ref = spec.graph_ref();
        self.transaction(|tx| {
            let existing: Option<String> = tx
                .inner
                .query_row(
                    "SELECT spec_digest FROM graphs WHERE graph_id = ?1 AND revision = ?2",
                    params![graph_ref.graph_id.as_str(), graph_ref.revision.get()],
                    |r| r.get(0),
                )
                .optional()?;
            match existing {
                Some(stored) if stored == digest => Ok(StoredGraph {
                    graph_ref: graph_ref.clone(),
                    digest: digest.clone(),
                    inserted: false,
                }),
                Some(stored) => Err(StoreError::RevisionConflict {
                    graph_ref: graph_ref.clone(),
                    stored,
                    offered: digest.clone(),
                }),
                None => {
                    tx.inner.execute(
                        "INSERT INTO graphs (graph_id, revision, spec_digest, spec_json) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            graph_ref.graph_id.as_str(),
                            graph_ref.revision.get(),
                            digest,
                            json
                        ],
                    )?;
                    Ok(StoredGraph {
                        graph_ref: graph_ref.clone(),
                        digest: digest.clone(),
                        inserted: true,
                    })
                }
            }
        })
    }

    /// Load a stored graph revision.
    pub fn load_graph(&self, graph_ref: &GraphRef) -> Result<Option<GraphSpec>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT spec_json FROM graphs WHERE graph_id = ?1 AND revision = ?2",
                params![graph_ref.graph_id.as_str(), graph_ref.revision.get()],
                |r| r.get(0),
            )
            .optional()?;
        json.map(|j| parse_record(&j).map_err(|e| StoreError::Corrupt(e.to_string())))
            .transpose()
    }

    /// Create a run of a stored graph revision and append its `run_created`
    /// event, atomically.
    pub fn create_run(&mut self, run: &GraphRun) -> Result<(), StoreError> {
        let json = serde_json::to_string(run).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        json_len_checked("graph run", &json)?;
        self.transaction(|tx| {
            let graph_exists: bool = tx.inner.query_row(
                "SELECT EXISTS (SELECT 1 FROM graphs WHERE graph_id = ?1 AND revision = ?2)",
                params![run.graph_ref.graph_id.as_str(), run.graph_ref.revision.get()],
                |r| r.get(0),
            )?;
            if !graph_exists {
                return Err(StoreError::MissingReference(format!(
                    "graph {}@{}",
                    run.graph_ref.graph_id, run.graph_ref.revision
                )));
            }
            let inserted = tx.inner.execute(
                "INSERT OR IGNORE INTO runs (run_id, graph_id, revision, budget_lineage_ref, run_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    run.run_id.as_str(),
                    run.graph_ref.graph_id.as_str(),
                    run.graph_ref.revision.get(),
                    run.budget_lineage_ref.as_ref().map(|r| r.as_str()),
                    json
                ],
            )?;
            if inserted == 0 {
                return Err(StoreError::Duplicate(format!("run {}", run.run_id)));
            }
            tx.append_event(
                &run.run_id,
                None,
                "run_created",
                &serde_json::json!({
                    "graph_id": run.graph_ref.graph_id,
                    "revision": run.graph_ref.revision,
                    "budget_lineage_ref": run.budget_lineage_ref,
                }),
            )?;
            Ok(())
        })
    }

    /// Load a stored run.
    pub fn load_run(&self, run_id: &RunId) -> Result<Option<GraphRun>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT run_json FROM runs WHERE run_id = ?1",
                params![run_id.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        json.map(|j| parse_record(&j).map_err(|e| StoreError::Corrupt(e.to_string())))
            .transpose()
    }

    /// Seal a finished attempt: insert its record and append `attempt_sealed`
    /// in one transaction.
    pub fn seal_attempt(&mut self, attempt: &Attempt) -> Result<(), StoreError> {
        self.transaction(|tx| {
            tx.insert_attempt(attempt)?;
            tx.append_event(
                &attempt.run_id,
                Some(&attempt.node.node_id),
                "attempt_sealed",
                &serde_json::json!({
                    "number": attempt.number,
                    "outcome": attempt.execution.as_ref().map(|e| e.outcome),
                    "result_digest": attempt.result.as_ref().map(|r| r.digest.clone()),
                }),
            )?;
            Ok(())
        })
    }

    /// Admit a verifier receipt for a sealed attempt: insert the receipt row
    /// and append `receipt_admitted` in one transaction. Acceptance cannot be
    /// recorded any other way.
    pub fn admit_receipt(&mut self, receipt: &VerifierReceipt) -> Result<(), StoreError> {
        self.transaction(|tx| {
            tx.insert_receipt(receipt)?;
            tx.append_event(
                &receipt.attempt.run_id,
                Some(&receipt.attempt.node.node_id),
                "receipt_admitted",
                &serde_json::json!({
                    "receipt_id": receipt.id,
                    "number": receipt.attempt.number,
                    "verdict": receipt.verdict,
                    "result_digest": receipt.result_digest,
                }),
            )?;
            Ok(())
        })
    }

    /// Record a gate packet and append `gate_opened` in one transaction.
    pub fn open_gate(&mut self, packet: &GatePacket) -> Result<(), StoreError> {
        self.transaction(|tx| {
            tx.insert_gate_packet(packet)?;
            tx.append_event(
                &packet.run_id,
                Some(&packet.node.node_id),
                "gate_opened",
                &serde_json::json!({ "packet_id": packet.id, "purpose": packet.purpose }),
            )?;
            Ok(())
        })
    }

    /// Record a gate decision for a stored packet and append
    /// `decision_admitted` in one transaction. One decision per packet.
    pub fn admit_decision(
        &mut self,
        run_id: &RunId,
        decision: &GateDecision,
    ) -> Result<(), StoreError> {
        self.transaction(|tx| {
            tx.insert_gate_decision(run_id, decision)?;
            tx.append_event(
                run_id,
                Some(&decision.scope.node.node_id),
                "decision_admitted",
                &serde_json::json!({
                    "packet_id": decision.gate_packet_id,
                    "decision": decision.decision,
                }),
            )?;
            Ok(())
        })
    }

    /// The event log of a run, in order.
    pub fn events(&self, run_id: &RunId) -> Result<Vec<StoredEvent>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, run_id, node_id, kind, payload_json, recorded_at FROM events WHERE run_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![run_id.as_str()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (seq, run, node, kind, payload, recorded_at) = row?;
            events.push(StoredEvent {
                seq: u64::try_from(seq).map_err(|e| StoreError::Corrupt(e.to_string()))?,
                run_id: RunId::new(run).map_err(|e| StoreError::Corrupt(e.to_string()))?,
                node_id: node
                    .map(|n| NodeId::new(n).map_err(|e| StoreError::Corrupt(e.to_string())))
                    .transpose()?,
                kind,
                payload: serde_json::from_str(&payload)
                    .map_err(|e| StoreError::Corrupt(e.to_string()))?,
                recorded_at,
            });
        }
        Ok(events)
    }

    /// Sealed attempts of one node in a run, in attempt order.
    pub fn attempts(&self, run_id: &RunId, node_id: &NodeId) -> Result<Vec<Attempt>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT attempt_json FROM attempts WHERE run_id = ?1 AND node_id = ?2 ORDER BY number",
        )?;
        let rows = stmt.query_map(params![run_id.as_str(), node_id.as_str()], |r| {
            r.get::<_, String>(0)
        })?;
        rows.map(|row| {
            let json = row?;
            parse_record(&json).map_err(|e| StoreError::Corrupt(e.to_string()))
        })
        .collect()
    }

    /// One sealed attempt by number.
    pub fn attempt(
        &self,
        run_id: &RunId,
        node_id: &NodeId,
        number: u32,
    ) -> Result<Option<Attempt>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT attempt_json FROM attempts WHERE run_id = ?1 AND node_id = ?2 AND number = ?3",
                params![run_id.as_str(), node_id.as_str(), number],
                |r| r.get(0),
            )
            .optional()?;
        json.map(|j| parse_record(&j).map_err(|e| StoreError::Corrupt(e.to_string())))
            .transpose()
    }

    /// Revoke a stored receipt for future work by appending a run-level
    /// `receipt_revoked` event. The receipt row and every earlier acceptance
    /// stay exactly as recorded.
    pub fn revoke_receipt(
        &mut self,
        run_id: &RunId,
        receipt_id: &str,
        reason: &str,
    ) -> Result<(), StoreError> {
        self.transaction(|tx| {
            let exists: bool = tx.inner.query_row(
                "SELECT EXISTS (SELECT 1 FROM receipts WHERE run_id = ?1 AND receipt_id = ?2)",
                params![run_id.as_str(), receipt_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(StoreError::MissingReference(format!(
                    "receipt {receipt_id}"
                )));
            }
            tx.append_event(
                run_id,
                None,
                "receipt_revoked",
                &serde_json::json!({ "receipt_id": receipt_id, "reason": reason }),
            )?;
            Ok(())
        })
    }

    /// Ids of receipts revoked in a run, from its event log.
    pub fn revoked_receipts(
        &self,
        run_id: &RunId,
    ) -> Result<std::collections::BTreeSet<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT json_extract(payload_json, '$.receipt_id') FROM events WHERE run_id = ?1 AND kind = 'receipt_revoked'",
        )?;
        let rows = stmt.query_map(params![run_id.as_str()], |r| r.get::<_, Option<String>>(0))?;
        let mut out = std::collections::BTreeSet::new();
        for row in rows {
            if let Some(id) = row? {
                out.insert(id);
            }
        }
        Ok(out)
    }

    /// One receipt by id.
    pub fn receipt(
        &self,
        run_id: &RunId,
        receipt_id: &str,
    ) -> Result<Option<VerifierReceipt>, StoreError> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT receipt_json FROM receipts WHERE run_id = ?1 AND receipt_id = ?2",
                params![run_id.as_str(), receipt_id],
                |r| r.get(0),
            )
            .optional()?;
        json.map(|j| parse_record(&j).map_err(|e| StoreError::Corrupt(e.to_string())))
            .transpose()
    }

    /// Per-node counts and last verdict for a run: a view derived from the
    /// records, never a source of truth.
    pub fn summary(&self, run_id: &RunId) -> Result<Vec<NodeSummary>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.node_id,
                    COUNT(DISTINCT a.number),
                    COUNT(r.receipt_id),
                    (SELECT verdict FROM receipts r2
                      WHERE r2.run_id = a.run_id AND r2.node_id = a.node_id
                      ORDER BY r2.rowid DESC LIMIT 1)
               FROM attempts a
               LEFT JOIN receipts r ON r.run_id = a.run_id AND r.node_id = a.node_id AND r.number = a.number
              WHERE a.run_id = ?1
              GROUP BY a.node_id
              ORDER BY a.node_id",
        )?;
        let rows = stmt.query_map(params![run_id.as_str()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (node, attempts, receipts, last) = row?;
            Ok(NodeSummary {
                node_id: NodeId::new(node).map_err(|e| StoreError::Corrupt(e.to_string()))?,
                attempts: u64::try_from(attempts).unwrap_or_default(),
                receipts: u64::try_from(receipts).unwrap_or_default(),
                last_verdict: last,
            })
        })
        .collect()
    }

    /// Row counts of every table, for retention accounting.
    pub fn row_counts(&self) -> Result<Vec<(String, u64)>, StoreError> {
        let mut out = Vec::new();
        for table in [
            "graphs",
            "runs",
            "events",
            "attempts",
            "receipts",
            "gate_packets",
            "gate_decisions",
            "artifacts",
        ] {
            let n: i64 =
                self.conn
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
            out.push((table.to_string(), u64::try_from(n).unwrap_or_default()));
        }
        Ok(out)
    }
}

impl Tx<'_> {
    /// Append one event to the run's log and return its sequence number.
    pub fn append_event(
        &self,
        run_id: &RunId,
        node_id: Option<&NodeId>,
        kind: &str,
        payload: &Value,
    ) -> Result<u64, StoreError> {
        let text = payload.to_string();
        json_len_checked("event payload", &text)?;
        self.inner
            .execute(
                "INSERT INTO events (run_id, node_id, kind, payload_json) VALUES (?1, ?2, ?3, ?4)",
                params![run_id.as_str(), node_id.map(NodeId::as_str), kind, text],
            )
            .map_err(|e| {
                if is_constraint(&e) {
                    StoreError::MissingReference(format!(
                        "run {run_id} (or the row {kind} requires)"
                    ))
                } else {
                    StoreError::Sqlite(e)
                }
            })?;
        Ok(u64::try_from(self.inner.last_insert_rowid()).unwrap_or_default())
    }

    /// Insert a sealed attempt record. Every started attempt is written once.
    pub fn insert_attempt(&self, attempt: &Attempt) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(attempt).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        json_len_checked("attempt", &json)?;
        let result = self.inner.execute(
            "INSERT INTO attempts (run_id, node_id, number, attempt_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                attempt.run_id.as_str(),
                attempt.node.node_id.as_str(),
                attempt.number.get(),
                json
            ],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if is_constraint(&e) => {
                let run_exists: bool = self.inner.query_row(
                    "SELECT EXISTS (SELECT 1 FROM runs WHERE run_id = ?1)",
                    params![attempt.run_id.as_str()],
                    |r| r.get(0),
                )?;
                if run_exists {
                    Err(StoreError::Duplicate(format!(
                        "attempt {}",
                        attempt.attempt_ref()
                    )))
                } else {
                    Err(StoreError::MissingReference(format!(
                        "run {}",
                        attempt.run_id
                    )))
                }
            }
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Insert a receipt row for a sealed attempt. This does not admit it; a
    /// `receipt_admitted` event in the same transaction does.
    pub fn insert_receipt(&self, receipt: &VerifierReceipt) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(receipt).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        json_len_checked("receipt", &json)?;
        let result = self.inner.execute(
            "INSERT INTO receipts (run_id, receipt_id, node_id, number, verdict, receipt_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                receipt.attempt.run_id.as_str(),
                receipt.id.as_str(),
                receipt.attempt.node.node_id.as_str(),
                receipt.attempt.number.get(),
                receipt.verdict.as_str(),
                json
            ],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if is_constraint(&e) => {
                let attempt_exists: bool = self.inner.query_row(
                    "SELECT EXISTS (SELECT 1 FROM attempts WHERE run_id = ?1 AND node_id = ?2 AND number = ?3)",
                    params![
                        receipt.attempt.run_id.as_str(),
                        receipt.attempt.node.node_id.as_str(),
                        receipt.attempt.number.get()
                    ],
                    |r| r.get(0),
                )?;
                if attempt_exists {
                    Err(StoreError::Duplicate(format!("receipt {}", receipt.id)))
                } else {
                    Err(StoreError::MissingReference(format!(
                        "attempt {}",
                        receipt.attempt
                    )))
                }
            }
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Insert a gate packet row.
    pub fn insert_gate_packet(&self, packet: &GatePacket) -> Result<(), StoreError> {
        let json = serde_json::to_string(packet).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        json_len_checked("gate packet", &json)?;
        self.inner
            .execute(
                "INSERT INTO gate_packets (run_id, packet_id, node_id, packet_json) VALUES (?1, ?2, ?3, ?4)",
                params![
                    packet.run_id.as_str(),
                    packet.id.as_str(),
                    packet.node.node_id.as_str(),
                    json
                ],
            )
            .map_err(|e| {
                if is_constraint(&e) {
                    StoreError::Duplicate(format!("gate packet {} (or its run is missing)", packet.id))
                } else {
                    StoreError::Sqlite(e)
                }
            })?;
        Ok(())
    }

    /// Insert a gate decision row for a stored packet; one per packet.
    pub fn insert_gate_decision(
        &self,
        run_id: &RunId,
        decision: &GateDecision,
    ) -> Result<(), StoreError> {
        let json =
            serde_json::to_string(decision).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        json_len_checked("gate decision", &json)?;
        let result = self.inner.execute(
            "INSERT INTO gate_decisions (run_id, packet_id, decision, decision_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                run_id.as_str(),
                decision.gate_packet_id.as_str(),
                decision.decision.as_str(),
                json
            ],
        );
        match result {
            Ok(_) => Ok(()),
            Err(e) if is_constraint(&e) => {
                let packet_exists: bool = self.inner.query_row(
                    "SELECT EXISTS (SELECT 1 FROM gate_packets WHERE run_id = ?1 AND packet_id = ?2)",
                    params![run_id.as_str(), decision.gate_packet_id.as_str()],
                    |r| r.get(0),
                )?;
                if packet_exists {
                    Err(StoreError::Duplicate(format!(
                        "decision for gate packet {}",
                        decision.gate_packet_id
                    )))
                } else {
                    Err(StoreError::MissingReference(format!(
                        "gate packet {}",
                        decision.gate_packet_id
                    )))
                }
            }
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Record a bounded reference to an artifact kept outside the store.
    pub fn insert_artifact(
        &self,
        run_id: &RunId,
        artifact: &ArtifactRef,
    ) -> Result<(), StoreError> {
        if artifact.locator.is_empty() || artifact.locator.len() > MAX_LOCATOR_LEN {
            return Err(StoreError::TooLarge {
                what: "artifact locator",
                size: artifact.locator.len(),
                limit: MAX_LOCATOR_LEN,
            });
        }
        self.inner
            .execute(
                "INSERT INTO artifacts (run_id, digest, kind, locator, size_bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    run_id.as_str(),
                    artifact.digest,
                    artifact.kind,
                    artifact.locator,
                    i64::try_from(artifact.size_bytes).unwrap_or(i64::MAX)
                ],
            )
            .map_err(|e| {
                if is_constraint(&e) {
                    StoreError::Duplicate(format!("artifact {} (or its run is missing)", artifact.digest))
                } else {
                    StoreError::Sqlite(e)
                }
            })?;
        Ok(())
    }

    /// Number of rows in `table`, inside this transaction.
    pub fn count(&self, table: &str) -> Result<u64, StoreError> {
        let n: i64 = self
            .inner
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or_default())
    }
}
