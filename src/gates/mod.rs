//! Build and open bounded human gate packets.
//!
//! A gate packet stops undecidable or exhausted work with everything a
//! human authority needs and nothing they do not: the sealed attempt, the
//! rejection or undecidable receipt, the result artifact, the original
//! obligation, the bounded options, and an expiry. It references sealed
//! context by digest; it never waits for the paused node's acceptance, and
//! nothing in it can carry a verdict.

use std::fmt;

use serde::Serialize;

use crate::contracts::{
    ContextKind, ContextRef, DecisionKind, GatePacket, GatePurpose, GraphRun, GraphSpec, Ident,
    NodeRef, PolicyRef, RecordKind, RequestedDecision, RunId, SchemaVersion, Timestamp, Version,
};
use crate::digests::{artifact_digest, digest_of, jcs_bytes};
use crate::state::{NodeEvent, RunState, kinds};
use crate::store::{Ledger, Store, StoreError};

/// The policy naming who may decide a controller-opened packet.
pub const OPERATOR_POLICY_ID: &str = "operator_decides";

/// Envelope kind of a packet subject digest.
pub const GATE_SUBJECT_KIND: &str = "checkspan.gate_subject@1";

/// Why a packet could not be built. Nothing was written.
#[derive(Debug)]
pub enum GateBuildError {
    /// The node is not in a state a resolve-work packet applies to.
    NotGateable {
        /// What the node's state is.
        state: String,
    },
    /// The node has no sealed attempt to present.
    NoSealedAttempt,
    /// The expiry is not after the packet's creation.
    ExpiryNotAfterCreation,
    /// The store refused.
    Store(StoreError),
    /// The packet could not be sealed.
    Internal(String),
}

impl fmt::Display for GateBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateBuildError::NotGateable { state } => {
                write!(
                    f,
                    "the node is not awaiting a resolve-work decision ({state})"
                )
            }
            GateBuildError::NoSealedAttempt => f.write_str("the node has no sealed attempt"),
            GateBuildError::ExpiryNotAfterCreation => {
                f.write_str("the packet would expire before it exists")
            }
            GateBuildError::Store(e) => write!(f, "{e}"),
            GateBuildError::Internal(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for GateBuildError {}

impl From<StoreError> for GateBuildError {
    fn from(e: StoreError) -> Self {
        GateBuildError::Store(e)
    }
}

/// What the packet's subject digest covers: everything the decision must
/// bind to. The digest field itself is excluded by construction.
#[derive(Serialize)]
struct Subject<'a> {
    run_id: &'a RunId,
    node: &'a NodeRef,
    purpose: GatePurpose,
    obligation: &'a str,
    context_refs: &'a [ContextRef],
    options: &'a [DecisionKind],
    authority_policy_ref: &'a PolicyRef,
    created_at: &'a Timestamp,
    expires_at: &'a Timestamp,
}

/// Build and record a resolve-work packet for a node whose work is
/// exhausted (rejected or failed with no attempts left) or undecidable.
/// The packet and its `gate_opened` event are written in one transaction;
/// the node becomes `gated`, waiting on the packet.
pub fn open_resolve_work(
    store: &mut Store,
    spec: &GraphSpec,
    run: &GraphRun,
    node_id: &crate::contracts::NodeId,
    now: &Timestamp,
    expires_at: &Timestamp,
) -> Result<GatePacket, GateBuildError> {
    if !now.is_before(expires_at) {
        return Err(GateBuildError::ExpiryNotAfterCreation);
    }
    let node = spec
        .nodes
        .iter()
        .find(|n| &n.id == node_id)
        .ok_or_else(|| GateBuildError::Internal(format!("node {node_id} is not in the graph")))?;
    let node_ref = spec
        .node_ref(node_id)
        .expect("a graph node has a reference");

    let mut failure: Option<GateBuildError> = None;
    let outcome = store.transaction(|tx| {
        let mut refuse = |error: GateBuildError| {
            failure = Some(error);
            Err(StoreError::Corrupt("gate refused".into()))
        };
        let events = tx.events(&run.run_id)?;
        let state = match RunState::replay(run.run_id.clone(), spec, &events) {
            Ok(state) => state,
            Err(e) => return refuse(GateBuildError::Internal(e.to_string())),
        };
        let Some(node_state) = state.node(node_id) else {
            return refuse(GateBuildError::Internal(format!(
                "node {node_id} has no state"
            )));
        };
        let Some(number) = node_state.current_attempt else {
            return refuse(GateBuildError::NoSealedAttempt);
        };
        let Some(attempt) = tx.attempt(&run.run_id, node_id, number.get())? else {
            return refuse(GateBuildError::NoSealedAttempt);
        };

        // Sealed context: the attempt, the deciding receipt when one was
        // admitted, and the result artifact when one exists.
        let mut context_refs = vec![ContextRef {
            kind: ContextKind::Attempt,
            id: attempt.attempt_ref().to_string(),
            digest: digest_of_record(&attempt).map_err(StoreError::Corrupt)?,
        }];
        let last_receipt_id = events
            .iter()
            .rev()
            .find(|e| e.kind == kinds::RECEIPT_ADMITTED && e.node_id.as_ref() == Some(node_id))
            .and_then(|e| e.payload.get("receipt_id").and_then(|v| v.as_str()))
            .map(str::to_owned);
        if let Some(receipt_id) = last_receipt_id
            && let Some(receipt) = tx.receipt(&run.run_id, &receipt_id)?
        {
            context_refs.push(ContextRef {
                kind: ContextKind::Receipt,
                id: receipt_id,
                digest: digest_of_record(&receipt).map_err(StoreError::Corrupt)?,
            });
        }
        if let Some(result) = &attempt.result {
            context_refs.push(ContextRef {
                kind: ContextKind::Artifact,
                id: result.artifact_ref.clone(),
                digest: result.digest.clone(),
            });
        }

        let options = vec![
            DecisionKind::Retry,
            DecisionKind::Revise,
            DecisionKind::Cancel,
        ];
        let authority = PolicyRef {
            id: Ident::new(OPERATOR_POLICY_ID).expect("an ident"),
            version: Version::new(1).expect("a version"),
        };
        let packet_id = Ident::new(format!("gate-{node_id}-{}", number.get()))
            .map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let subject_digest = digest_of(
            GATE_SUBJECT_KIND,
            &Subject {
                run_id: &run.run_id,
                node: &node_ref,
                purpose: GatePurpose::ResolveWork,
                obligation: &node.acceptance.claim,
                context_refs: &context_refs,
                options: &options,
                authority_policy_ref: &authority,
                created_at: now,
                expires_at,
            },
        )
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let packet = GatePacket {
            record: RecordKind::GatePacket,
            schema_version: SchemaVersion(1),
            id: packet_id,
            run_id: run.run_id.clone(),
            node: node_ref.clone(),
            purpose: GatePurpose::ResolveWork,
            context_refs,
            subject_digest,
            requested_decision: RequestedDecision {
                options,
                action: None,
            },
            authority_policy_ref: authority,
            created_at: now.clone(),
            expires_at: expires_at.clone(),
        };
        let bindings = packet.check_bindings();
        if !bindings.is_empty() {
            let text = bindings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return refuse(GateBuildError::Internal(text));
        }
        // The transition must be legal from the node's current state:
        // rejected or failed work, or a gated node with no packet yet.
        if let Err(e) = node_state.apply(&NodeEvent::GateOpened {
            packet: packet.id.clone(),
        }) {
            return refuse(GateBuildError::NotGateable {
                state: format!("{}: {e}", node_state.status.as_str()),
            });
        }
        tx.insert_gate_packet(&packet)?;
        tx.append_event(
            &run.run_id,
            Some(node_id),
            kinds::GATE_OPENED,
            &serde_json::json!({ "packet_id": packet.id, "purpose": packet.purpose }),
        )?;
        Ok(packet)
    });
    match outcome {
        Ok(packet) => Ok(packet),
        Err(e) => Err(failure.unwrap_or(GateBuildError::Store(e))),
    }
}

/// Digest of one record's canonical bytes.
fn digest_of_record<T: Serialize>(record: &T) -> Result<crate::contracts::Digest, String> {
    let value = serde_json::to_value(record).map_err(|e| e.to_string())?;
    let bytes = jcs_bytes(&value).map_err(|e| e.to_string())?;
    Ok(artifact_digest(&bytes))
}
