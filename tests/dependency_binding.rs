//! Run-scoped dependency resolution: an older run's green node cannot
//! silently satisfy a new run, explicit imports can, and changed, expired,
//! or revoked dependencies block fresh work while historical receipts and
//! acceptances stay exactly as recorded.

mod common;

use std::fs;

use checkspan::contracts::Record;
use checkspan::contracts::{
    Attempt, Digest, GraphRun, GraphSpec, ImportedReceipt, NodeId, NodeStatus, ReceiptRef,
    RecordKind, RunId, SchemaVersion, Timestamp, TypeRef, VerifierReceipt, parse_record,
};
use checkspan::deps::{Blocker, DependencyService, Provenance};
use checkspan::digests::artifact_digest;
use checkspan::state::{RunState, kinds};
use checkspan::store::Store;
use common::fixture;
use serde_json::json;

const NOW: &str = "2026-09-06T15:00:00Z";
const LATER: &str = "2026-10-02T00:00:00Z";

fn ts(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}

fn node(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

fn run_record(run_id: &str, spec: &GraphSpec, imports: Vec<ImportedReceipt>) -> GraphRun {
    GraphRun {
        record: RecordKind::GraphRun,
        schema_version: SchemaVersion(1),
        run_id: RunId::new(run_id).unwrap(),
        graph_ref: spec.graph_ref(),
        budget_lineage_ref: None,
        admitted_imports: imports,
    }
}

/// A fresh store with the full review graph and run-0001, in which cs_patch
/// (revision 1) is dispatched, sealed with a `patch_result@1` result, and
/// accepted by receipt `rcpt-patch-1` valid until 2026-10-01.
fn seeded(name: &str) -> (Store, GraphSpec, GraphRun, Digest) {
    let dir = std::env::temp_dir().join(format!("checkspan-deps-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut store = Store::open(dir.join("ledger.sqlite")).unwrap();
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    store.store_graph(&spec).unwrap();
    let run = run_record("run-0001", &spec, vec![]);
    store.create_run(&run).unwrap();

    let patch = node("cs_patch");
    let result_digest = artifact_digest(b"the exact patch bytes");
    store
        .transaction(|tx| {
            tx.append_event(
                &run.run_id,
                Some(&patch),
                kinds::ATTEMPT_DISPATCHED,
                &json!({"number": 1}),
            )?;
            Ok(())
        })
        .unwrap();
    let mut attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    attempt.node = spec.node_ref(&patch).unwrap();
    attempt.verifier_receipt = None;
    attempt.dependency_receipts.clear();
    let result = attempt.result.as_mut().unwrap();
    result.result_type = spec.nodes[0].result_type.clone();
    result.digest = result_digest.clone();
    store.seal_attempt(&attempt).unwrap();

    let mut receipt: VerifierReceipt =
        parse_record(&fixture("outcomes/valid/receipt-accept.json")).unwrap();
    receipt.id = checkspan::contracts::Ident::new("rcpt-patch-1").unwrap();
    receipt.attempt = attempt.attempt_ref();
    receipt.result_digest = result_digest.clone();
    receipt.validity.valid_until = Some(ts("2026-10-01T00:00:00Z"));
    store.admit_receipt(&receipt).unwrap();
    (store, spec, run, result_digest)
}

fn state_of(store: &Store, spec: &GraphSpec, run: &GraphRun) -> RunState {
    RunState::replay(
        run.run_id.clone(),
        spec,
        &store.events(&run.run_id).unwrap(),
    )
    .unwrap()
}

fn patch_type(spec: &GraphSpec) -> TypeRef {
    spec.nodes[0].result_type.clone()
}

#[test]
fn a_same_run_dependency_resolves_to_the_exact_receipt() {
    let (store, spec, run, digest) = seeded("same-run");
    let state = state_of(&store, &spec, &run);
    assert_eq!(
        state.node(&node("cs_patch")).unwrap().status,
        NodeStatus::Accepted
    );
    let now = ts(NOW);
    let service = DependencyService::new(&spec, &run, &state, &store, &now);

    let snapshot = service.resolve(&node("cs_ci")).unwrap();
    assert_eq!(snapshot.resolved.len(), 1);
    let resolved = &snapshot.resolved[0];
    assert_eq!(resolved.provenance, Provenance::SameRun);
    assert_eq!(resolved.receipt.id.as_str(), "rcpt-patch-1");
    assert_eq!(resolved.receipt.run_id.as_str(), "run-0001");
    assert_eq!(
        resolved.receipt.node,
        spec.node_ref(&node("cs_patch")).unwrap()
    );
    assert_eq!(resolved.result_digest, digest);
    assert_eq!(snapshot.receipt_refs().len(), 1);
    assert!(service.blocked_reason(&node("cs_ci")).is_none());

    let blockers = service.resolve(&node("cs_packet")).unwrap_err();
    assert_eq!(blockers.len(), 1, "{blockers:?}");
    assert!(matches!(
        &blockers[0],
        Blocker::NotAccepted { node, status: NodeStatus::Open } if node.as_str() == "cs_ci"
    ));
    let reason = service.blocked_reason(&node("cs_packet")).unwrap();
    assert!(
        reason.contains("cs_ci") && reason.contains("not accepted"),
        "{reason}"
    );
    assert!(service.recheck(&snapshot).is_ok());
    assert!(
        service
            .recheck_refs(&node("cs_ci"), &snapshot.receipt_refs())
            .is_ok()
    );
}

#[test]
fn an_older_runs_green_node_cannot_satisfy_a_new_run() {
    let (mut store, spec, _run1, _) = seeded("older-run");
    let run2 = run_record("run-0002", &spec, vec![]);
    store.create_run(&run2).unwrap();
    let state = state_of(&store, &spec, &run2);
    assert_eq!(
        state.node(&node("cs_patch")).unwrap().status,
        NodeStatus::Open
    );
    let now = ts(NOW);
    let service = DependencyService::new(&spec, &run2, &state, &store, &now);
    let blockers = service.resolve(&node("cs_ci")).unwrap_err();
    assert!(
        matches!(
            blockers.as_slice(),
            [Blocker::NotAccepted { node, status: NodeStatus::Open }] if node.as_str() == "cs_patch"
        ),
        "{blockers:?}"
    );
    assert!(
        store
            .receipt(&RunId::new("run-0001").unwrap(), "rcpt-patch-1")
            .unwrap()
            .is_some(),
        "run-0001's receipt is still there; it just does not count here"
    );
}

#[test]
fn an_explicit_import_satisfies_the_dependency_only_when_it_matches_exactly() {
    let (mut store, spec, _run1, digest) = seeded("import");
    let receipt = ReceiptRef {
        id: checkspan::contracts::Ident::new("rcpt-patch-1").unwrap(),
        run_id: RunId::new("run-0001").unwrap(),
        node: spec.node_ref(&node("cs_patch")).unwrap(),
    };
    let import = ImportedReceipt {
        node_id: node("cs_patch"),
        receipt: receipt.clone(),
        result_digest: digest.clone(),
        result_type: patch_type(&spec),
        subject: "commit:918a8431ec8d46b19029e3ecbbb77e785b25716c".into(),
        admitted_at: ts(NOW),
        valid_until: None,
    };
    let run3 = run_record("run-0003", &spec, vec![import.clone()]);
    assert_eq!(run3.check_identity(), vec![]);
    store.create_run(&run3).unwrap();
    let state = state_of(&store, &spec, &run3);
    let now = ts(NOW);
    let service = DependencyService::new(&spec, &run3, &state, &store, &now);
    let snapshot = service.resolve(&node("cs_ci")).unwrap();
    assert_eq!(snapshot.resolved[0].provenance, Provenance::Imported);
    assert_eq!(snapshot.resolved[0].receipt, receipt);
    assert_eq!(snapshot.resolved[0].result_digest, digest);

    type Expect = fn(&Blocker) -> bool;
    let cases: Vec<(&str, ImportedReceipt, Expect)> = vec![
        (
            "wrong digest",
            ImportedReceipt {
                result_digest: artifact_digest(b"other"),
                ..import.clone()
            },
            |b| matches!(b, Blocker::SubjectMismatch { .. }),
        ),
        (
            "wrong result type",
            ImportedReceipt {
                result_type: spec.nodes[1].result_type.clone(),
                ..import.clone()
            },
            |b| matches!(b, Blocker::ResultTypeMismatch { .. }),
        ),
        (
            "unknown receipt",
            ImportedReceipt {
                receipt: ReceiptRef {
                    id: checkspan::contracts::Ident::new("rcpt-nope").unwrap(),
                    ..receipt.clone()
                },
                ..import.clone()
            },
            |b| matches!(b, Blocker::ReceiptMissing(_)),
        ),
        (
            "expired import",
            ImportedReceipt {
                valid_until: Some(ts("2026-09-01T00:00:00Z")),
                ..import.clone()
            },
            |b| matches!(b, Blocker::Expired { .. }),
        ),
    ];
    for (label, bad_import, expected) in cases {
        let run = run_record("run-0004", &spec, vec![bad_import]);
        let state = RunState::new(run.run_id.clone(), &spec);
        let service = DependencyService::new(&spec, &run, &state, &store, &now);
        let blockers = service.resolve(&node("cs_ci")).unwrap_err();
        assert_eq!(blockers.len(), 1, "{label}: {blockers:?}");
        assert!(expected(&blockers[0]), "{label}: {blockers:?}");
    }

    // A receipt about another node revision cannot be imported for this one.
    let mut other_revision = spec.clone();
    other_revision.revision = checkspan::contracts::Revision::new(3).unwrap();
    other_revision.nodes[0].revision = checkspan::contracts::Revision::new(2).unwrap();
    for dep in other_revision
        .nodes
        .iter_mut()
        .flat_map(|n| n.deps.iter_mut())
    {
        if dep.node.node_id.as_str() == "cs_patch" {
            dep.node.revision = checkspan::contracts::Revision::new(2).unwrap();
        }
    }
    store.store_graph(&other_revision).unwrap();
    let run5 = run_record("run-0005", &other_revision, vec![import.clone()]);
    store.create_run(&run5).unwrap();
    let state = state_of(&store, &other_revision, &run5);
    let service = DependencyService::new(&other_revision, &run5, &state, &store, &now);
    let blockers = service.resolve(&node("cs_ci")).unwrap_err();
    assert!(
        matches!(
            blockers.as_slice(),
            [Blocker::NodeMismatch { expected, found }]
                if expected.revision.get() == 2 && found.revision.get() == 1
        ),
        "{blockers:?}"
    );
}

#[test]
fn import_identity_rules_reject_duplicates_and_self_imports() {
    let (_, spec, _, digest) = seeded("import-identity");
    let receipt = ReceiptRef {
        id: checkspan::contracts::Ident::new("rcpt-patch-1").unwrap(),
        run_id: RunId::new("run-0001").unwrap(),
        node: spec.node_ref(&node("cs_patch")).unwrap(),
    };
    let import = ImportedReceipt {
        node_id: node("cs_patch"),
        receipt,
        result_digest: digest,
        result_type: patch_type(&spec),
        subject: "commit:abc".into(),
        admitted_at: ts(NOW),
        valid_until: None,
    };
    let duplicate = run_record("run-0009", &spec, vec![import.clone(), import.clone()]);
    assert!(duplicate
        .check_identity()
        .iter()
        .any(|e| matches!(e, checkspan::contracts::IdentityError::DuplicateImport(n) if n.as_str() == "cs_patch")));
    let mut own = import.clone();
    own.receipt.run_id = RunId::new("run-0001").unwrap();
    let self_import = run_record("run-0001", &spec, vec![own]);
    assert!(
        self_import
            .check_identity()
            .iter()
            .any(|e| matches!(e, checkspan::contracts::IdentityError::SelfImport(_)))
    );
    let text = serde_json::to_string(&duplicate).unwrap();
    let again: GraphRun = parse_record(&text).unwrap();
    assert_eq!(again, duplicate, "imports round-trip through the record");
    assert!(common::schema_errors(checkspan::contracts::GraphRun::SCHEMA_ID, &text).is_empty());
}

#[test]
fn expired_receipts_block_fresh_work_but_history_stays() {
    let (store, spec, run, _) = seeded("expired");
    let state = state_of(&store, &spec, &run);
    let before = ts(NOW);
    let after = ts(LATER);
    let fresh = DependencyService::new(&spec, &run, &state, &store, &before);
    let snapshot = fresh.resolve(&node("cs_ci")).unwrap();
    let stale = DependencyService::new(&spec, &run, &state, &store, &after);
    let blockers = stale.resolve(&node("cs_ci")).unwrap_err();
    assert!(
        matches!(
            blockers.as_slice(),
            [Blocker::Expired { receipt, valid_until }]
                if receipt.id.as_str() == "rcpt-patch-1" && valid_until.as_str() == "2026-10-01T00:00:00Z"
        ),
        "{blockers:?}"
    );
    assert!(
        stale.recheck(&snapshot).is_err(),
        "a pinned snapshot is rechecked at the new moment"
    );
    assert_eq!(
        state.node(&node("cs_patch")).unwrap().status,
        NodeStatus::Accepted,
        "history unchanged"
    );
    assert!(
        store
            .receipt(&run.run_id, "rcpt-patch-1")
            .unwrap()
            .is_some()
    );
}

#[test]
fn revoked_receipts_block_fresh_work_but_history_stays() {
    let (mut store, spec, run, _) = seeded("revoked");
    let now = ts(NOW);
    let snapshot = {
        let state = state_of(&store, &spec, &run);
        let service = DependencyService::new(&spec, &run, &state, &store, &now);
        let snapshot = service.resolve(&node("cs_ci")).unwrap();
        assert!(service.recheck(&snapshot).is_ok());
        snapshot
    };
    assert!(matches!(
        store.revoke_receipt(&run.run_id, "rcpt-nope", "no such receipt"),
        Err(checkspan::store::StoreError::MissingReference(_))
    ));
    store
        .revoke_receipt(
            &run.run_id,
            "rcpt-patch-1",
            "attestation withdrawn by issuer",
        )
        .unwrap();
    let events = store.events(&run.run_id).unwrap();
    let revoked = events.last().unwrap();
    assert_eq!(revoked.kind, "receipt_revoked");
    assert!(revoked.node_id.is_none(), "revocation is a run-level event");
    assert_eq!(revoked.payload["receipt_id"], "rcpt-patch-1");
    assert_eq!(store.revoked_receipts(&run.run_id).unwrap().len(), 1);

    let state = state_of(&store, &spec, &run);
    assert_eq!(
        state.node(&node("cs_patch")).unwrap().status,
        NodeStatus::Accepted,
        "the historical acceptance stands"
    );
    let service = DependencyService::new(&spec, &run, &state, &store, &now);
    let blockers = service.resolve(&node("cs_ci")).unwrap_err();
    assert!(
        matches!(blockers.as_slice(), [Blocker::Revoked(r)] if r.id.as_str() == "rcpt-patch-1"),
        "{blockers:?}"
    );
    assert!(
        matches!(
            service.recheck(&snapshot).unwrap_err().as_slice(),
            [Blocker::Revoked(_)]
        ),
        "the snapshot pinned at dispatch no longer passes the recheck before acceptance"
    );
    assert!(matches!(
        service
            .recheck_refs(&node("cs_ci"), &snapshot.receipt_refs())
            .unwrap_err()
            .as_slice(),
        [Blocker::Revoked(_)]
    ));
    let receipt = store.receipt(&run.run_id, "rcpt-patch-1").unwrap().unwrap();
    assert_eq!(
        receipt.verdict,
        checkspan::contracts::Verdict::Accept,
        "the receipt row is untouched"
    );
}

#[test]
fn a_changed_subject_or_a_non_acceptance_never_resolves() {
    let (mut store, spec, run, digest) = seeded("changed");
    let now = ts(NOW);
    let ci = node("cs_ci");
    // The reducer state is taken before the extra records below; the
    // service reads receipts from the store, not from the state.
    let state = state_of(&store, &spec, &run);
    // Seal a second cs_patch attempt with a different result and a reject
    // receipt for it: neither a rejection nor the wrong digest can serve.
    let mut attempt: Attempt =
        parse_record(&fixture("outcomes/valid/attempt-retry-with-hint.json")).unwrap();
    attempt.node = spec.node_ref(&node("cs_patch")).unwrap();
    attempt.dependency_receipts.clear();
    let other = artifact_digest(b"a changed candidate");
    let result = attempt.result.as_mut().unwrap();
    result.result_type = patch_type(&spec);
    result.digest = other.clone();
    store.seal_attempt(&attempt).unwrap();
    let mut reject: VerifierReceipt =
        parse_record(&fixture("outcomes/valid/receipt-reject.json")).unwrap();
    reject.attempt = attempt.attempt_ref();
    reject.result_digest = other.clone();
    store.admit_receipt(&reject).unwrap();

    let service = DependencyService::new(&spec, &run, &state, &store, &now);
    let patch_ref = spec.node_ref(&node("cs_patch")).unwrap();
    let rejected = ReceiptRef {
        id: reject.id.clone(),
        run_id: run.run_id.clone(),
        node: patch_ref.clone(),
    };
    assert!(matches!(
        service.admissible(&rejected, &patch_ref, &patch_type(&spec), None),
        Err(Blocker::NotAnAcceptance {
            verdict: checkspan::contracts::Verdict::Reject,
            ..
        })
    ));
    let accepted = ReceiptRef {
        id: checkspan::contracts::Ident::new("rcpt-patch-1").unwrap(),
        run_id: run.run_id.clone(),
        node: patch_ref.clone(),
    };
    assert!(
        matches!(
            service.admissible(&accepted, &patch_ref, &patch_type(&spec), Some(&other)),
            Err(Blocker::SubjectMismatch { .. })
        ),
        "an import that expects the changed digest cannot use the old receipt"
    );
    assert_eq!(
        service
            .admissible(&accepted, &patch_ref, &patch_type(&spec), Some(&digest))
            .unwrap(),
        digest
    );
    let wrong_type = spec.nodes[1].result_type.clone();
    assert!(matches!(
        service.admissible(&accepted, &patch_ref, &wrong_type, None),
        Err(Blocker::ResultTypeMismatch { .. })
    ));
    // A pinned set with an extra receipt for an undeclared dependency fails.
    let mut pinned = service.resolve(&ci).unwrap().receipt_refs();
    pinned.push(rejected);
    assert!(service.recheck_refs(&ci, &pinned).is_err());
    assert!(
        service.recheck_refs(&ci, &[]).is_err(),
        "a missing pinned dependency fails"
    );
}
