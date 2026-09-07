//! The local run workflow: operate patch → check through one controller.
//!
//! Everything here composes what earlier prompts built — admission, the
//! store, the scheduler, evidence resolution, the verifier host, the two
//! built-in verifiers, receipts, and the retry policy — into commands the
//! CLI exposes. One `step` performs one action: verify a sealed attempt
//! that awaits its receipt, or claim and execute the next ready node.
//! Progress is durable; a fresh process continues exactly where the ledger
//! says the run is, never re-doing accepted work.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::{Value, json};

use crate::adapters::code::{Selection, Selector};
use crate::budget::{self, Disposition, Narrowing};
use crate::contracts::{
    Attempt, Execution, ExecutionOutcome, GraphSpec, Ident, NodeSpec, NodeStatus, RecordKind,
    RunId, SchemaVersion, Timestamp, Validity, parse_record,
};
use crate::deps::DependencyService;
use crate::evidence::{Item, Limits, Offer, Resolver, Source, Sources};
use crate::receipts;
use crate::scheduler::{Claim, ClaimOutcome, ControllerId, Scheduler};
use crate::state::RunState;
use crate::store::{Ledger, Store};
use crate::verifier_host::{self, ProcessFailure, VerifierProfile, VerifierRequest};
use crate::verifiers::{patch, software};

/// The source name a run's repository is registered under. Graphs written
/// for this controller scope their `code` ports to it.
pub const REPO_SOURCE: &str = "repo:local";

/// How long one verifier child may run.
const VERIFIER_TIMEOUT: Duration = Duration::from_secs(600);
/// Output ceilings for verifier children.
const VERIFIER_OUTPUT_CAP: usize = 4 << 20;

/// The development-default packet expiry; the CLI can override it.
pub const DEFAULT_GATE_EXPIRY: &str = "2027-01-01T00:00:00Z";

/// Everything one controller needs to operate a run.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The ledger file.
    pub store: PathBuf,
    /// The graph document. Its content is bound to the stored revision;
    /// editing it mid-run is refused.
    pub graph: PathBuf,
    /// The candidate repository, when the graph needs one.
    pub repo: Option<PathBuf>,
    /// The pinned software validation profile, when the graph needs one.
    pub software_profile: Option<PathBuf>,
    /// Where result artifacts are written.
    pub artifacts: PathBuf,
    /// This controller's identity.
    pub controller: String,
    /// When packets this controller opens stop accepting decisions.
    pub gate_expiry: String,
}

/// Why a run command failed. Reported as diagnostics; exit code 1.
#[derive(Debug)]
pub struct RunError(pub String);

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RunError {}

fn fail(message: impl Into<String>) -> RunError {
    RunError(message.into())
}

macro_rules! run_try {
    ($expr:expr, $what:literal) => {
        $expr.map_err(|e| fail(format!(concat!($what, ": {}"), e)))?
    };
}

/// Validate and admit the graph document, store it, and create the run.
pub fn create(store_path: &Path, graph: &Path, run_id: &str) -> Result<Value, RunError> {
    let spec = load_graph(graph)?;
    let run_id = run_try!(RunId::new(run_id), "run id");
    let mut store = run_try!(Store::open(store_path), "cannot open the store");
    run_try!(store.store_graph(&spec), "cannot store the graph");
    let run = crate::contracts::GraphRun {
        record: RecordKind::GraphRun,
        schema_version: SchemaVersion(1),
        run_id: run_id.clone(),
        graph_ref: spec.graph_ref(),
        budget_lineage_ref: None,
        admitted_imports: vec![],
    };
    run_try!(store.create_run(&run), "cannot create the run");
    Ok(json!({
        "command": "run-create",
        "run_id": run_id,
        "graph": { "graph_id": spec.graph_id, "revision": spec.revision },
        "nodes": spec.nodes.len(),
    }))
}

/// The run's nodes, budget, and active claims, as the ledger has them.
pub fn status(
    store_path: &Path,
    graph: &Path,
    run_id: &str,
    now: &Timestamp,
) -> Result<Value, RunError> {
    let spec = load_graph(graph)?;
    let (store, run) = open_run(store_path, &spec, run_id)?;
    let controller = run_try!(ControllerId::new("status"), "controller id");
    let scheduler = run_try!(Scheduler::new(&spec, &run, controller), "scheduler");
    let views = run_try!(scheduler.views(&store, now), "views");
    let budget = run_try!(
        budget::budget_status(&store, &spec, &run, now),
        "budget status"
    );
    let claims: Vec<Value> = run_try!(store.active_claims(), "claims")
        .into_iter()
        .filter(|c| c.run_id == run.run_id)
        .map(|c| {
            json!({
                "node": c.node_id,
                "number": c.number,
                "owner": c.owner,
                "fence": c.fence,
            })
        })
        .collect();
    Ok(json!({
        "command": "run-status",
        "run_id": run.run_id,
        "nodes": views,
        "budget": {
            "total_attempts": budget.total_attempts,
            "consumed": budget.consumed(),
            "remaining": budget.remaining(),
        },
        "active_claims": claims,
    }))
}

/// Cancel one node: its active claim is released and the node is cancelled.
pub fn cancel(
    store_path: &Path,
    graph: &Path,
    run_id: &str,
    node_id: &str,
    controller: &str,
) -> Result<Value, RunError> {
    let spec = load_graph(graph)?;
    let (mut store, run) = open_run(store_path, &spec, run_id)?;
    let controller = run_try!(ControllerId::new(controller), "controller id");
    let scheduler = run_try!(Scheduler::new(&spec, &run, controller), "scheduler");
    let node = run_try!(crate::contracts::NodeId::new(node_id), "node id");
    run_try!(scheduler.cancel(&mut store, &node), "cannot cancel");
    Ok(json!({
        "command": "run-cancel",
        "run_id": run.run_id,
        "node": node,
        "status": "cancelled",
    }))
}

/// Print the newest gate packet for one node, for the operator to review.
pub fn packet(
    store_path: &Path,
    graph: &Path,
    run_id: &str,
    node_id: &str,
) -> Result<Value, RunError> {
    let spec = load_graph(graph)?;
    let (store, run) = open_run(store_path, &spec, run_id)?;
    let packets = run_try!(store.gate_packets(&run.run_id), "cannot read packets");
    let newest = packets
        .into_iter()
        .rev()
        .find(|p| p.node.node_id.as_str() == node_id)
        .ok_or_else(|| fail(format!("no gate packet exists for {node_id}")))?;
    Ok(json!({
        "command": "run-packet",
        "run_id": run.run_id,
        "packet": newest,
    }))
}

/// Perform one action and report it.
pub fn step(ws: &Workspace, run_id: &str, now: &Timestamp) -> Result<Value, RunError> {
    let spec = load_graph(&ws.graph)?;
    let (mut store, run) = open_run(&ws.store, &spec, run_id)?;
    step_inner(ws, &spec, &mut store, &run, now)
}

/// Step until the run is complete, blocked, exhausted, or idle. Reports
/// every action taken.
pub fn drive(
    ws: &Workspace,
    run_id: &str,
    now: &Timestamp,
    max_steps: usize,
) -> Result<Value, RunError> {
    let spec = load_graph(&ws.graph)?;
    let (mut store, run) = open_run(&ws.store, &spec, run_id)?;
    let mut actions = Vec::new();
    for _ in 0..max_steps {
        let action = step_inner(ws, &spec, &mut store, &run, now)?;
        let stop = matches!(
            action.get("action").and_then(Value::as_str),
            Some("complete" | "idle" | "blocked" | "budget_exhausted")
        );
        actions.push(action);
        if stop {
            break;
        }
    }
    Ok(json!({
        "command": "run-drive",
        "run_id": run.run_id,
        "actions": actions,
    }))
}

fn step_inner(
    ws: &Workspace,
    spec: &GraphSpec,
    store: &mut Store,
    run: &crate::contracts::GraphRun,
    now: &Timestamp,
) -> Result<Value, RunError> {
    let controller = run_try!(ControllerId::new(&ws.controller), "controller id");
    let scheduler = run_try!(Scheduler::new(spec, run, controller), "scheduler");
    let events = run_try!(store.events(&run.run_id), "events");
    let state = run_try!(
        RunState::replay(run.run_id.clone(), spec, &events),
        "replay"
    );

    // 1. A sealed attempt awaiting its verdict is verified first; a fresh
    // process resumes verification instead of claiming more work.
    for node in &spec.nodes {
        let Some(node_state) = state.node(&node.id) else {
            continue;
        };
        if !node_state.checking {
            continue;
        }
        let number = node_state
            .current_attempt
            .expect("a checking node has an attempt")
            .get();
        let attempt = run_try!(store.attempt(&run.run_id, &node.id, number), "attempt")
            .ok_or_else(|| fail(format!("attempt {number} of {} is not sealed", node.id)))?;
        return verify_and_admit(ws, spec, store, run, node, &attempt, now);
    }

    // 2. Otherwise claim and execute the next ready node.
    match run_try!(scheduler.claim_next(store, now), "claim") {
        ClaimOutcome::Claimed(claim) => execute_claim(ws, spec, store, run, &claim, now),
        ClaimOutcome::Busy { active, max_active } => Ok(json!({
            "action": "idle",
            "reason": format!("{active} active claims at the cap of {max_active}"),
        })),
        ClaimOutcome::BudgetExhausted(reason) => Ok(json!({
            "action": "budget_exhausted",
            "reason": reason.to_string(),
        })),
        ClaimOutcome::NothingReady { blocked, conflicts } => {
            let complete = spec_targets_accepted(spec, &state);
            if complete {
                Ok(json!({ "action": "complete" }))
            } else if blocked.is_empty() && conflicts.is_empty() {
                Ok(json!({ "action": "idle", "reason": "nothing is ready" }))
            } else {
                Ok(json!({
                    "action": "blocked",
                    "blocked": blocked
                        .iter()
                        .map(|(node, reason)| json!({ "node": node, "reason": reason }))
                        .collect::<Vec<_>>(),
                    "conflicts": conflicts
                        .iter()
                        .map(|(node, resource)| json!({ "node": node, "resource": resource }))
                        .collect::<Vec<_>>(),
                }))
            }
        }
    }
}

fn spec_targets_accepted(spec: &GraphSpec, state: &RunState) -> bool {
    spec.targets.iter().all(|target| {
        state
            .node(&target.node_id)
            .map(|s| s.status == NodeStatus::Accepted)
            .unwrap_or(false)
    })
}

/// Execute one claimed node: resolve its evidence, produce or obtain its
/// result, seal the attempt, and verify it.
fn execute_claim(
    ws: &Workspace,
    spec: &GraphSpec,
    store: &mut Store,
    run: &crate::contracts::GraphRun,
    claim: &Claim,
    now: &Timestamp,
) -> Result<Value, RunError> {
    let node = spec
        .nodes
        .iter()
        .find(|n| n.id == claim.node_id)
        .expect("claimed nodes are in the graph");

    // Freeze the inputs.
    let resolved = resolve_evidence(ws, spec, store, run, node, claim, now);
    let manifest = match resolved {
        Ok(manifest) => manifest,
        Err(reason) => {
            let sealed = seal(
                store,
                &scheduler_for(ws, spec, run)?,
                claim,
                node,
                run,
                Vec::new(),
                None,
                ExecutionOutcome::Failed,
                Some(reason.clone()),
                now,
            )?;
            let disposition = retry(ws, store, spec, run, &claim.node_id, now)?;
            return Ok(json!({
                "action": "attempt_failed",
                "node": claim.node_id,
                "attempt": sealed.number,
                "reason": reason,
                "disposition": disposition,
            }));
        }
    };

    match node.result_type.schema_id.as_str() {
        // A task whose result is the captured candidate itself.
        "patch_result" => {
            let code = manifest
                .evidence
                .iter()
                .find(|r| r.bytes.is_some() && r.reference.subject.starts_with("patch_subject:"))
                .ok_or_else(|| fail("the patch task resolved no code evidence"))?;
            let bytes = code.bytes.clone().expect("code evidence carries bytes");
            let artifact = artifact_path(ws, run, &claim.node_id, claim.number.get())?;
            run_try!(
                std::fs::write(&artifact, &bytes),
                "cannot write the artifact"
            );
            let attempt = seal(
                store,
                &scheduler_for(ws, spec, run)?,
                claim,
                node,
                run,
                manifest.references(),
                Some((artifact.clone(), bytes)),
                ExecutionOutcome::Completed,
                None,
                now,
            )?;
            verify_and_admit(ws, spec, store, run, node, &attempt, now)
        }
        // A check whose result is the verifier's own typed report.
        "software_check_result" => {
            let subject = manifest_subject(&manifest.references())
                .ok_or_else(|| fail("the check resolved no code evidence"))?;
            let profile_path = ws
                .software_profile
                .as_ref()
                .ok_or_else(|| fail("no --software-profile was given"))?;
            let out = artifact_path(ws, run, &claim.node_id, claim.number.get())?;
            let request = request_for(
                node,
                run,
                claim,
                &subject,
                &manifest.references(),
                &manifest.digest,
            );
            let host = host_profile(ws, software::child_args(profile_path, &out))?;
            let completion = verifier_host::run(&host, &request, &Arc::new(AtomicBool::new(false)));
            match completion.outcome {
                Ok(response) => {
                    let bytes = run_try!(std::fs::read(&out), "cannot read the check result");
                    let attempt = seal(
                        store,
                        &scheduler_for(ws, spec, run)?,
                        claim,
                        node,
                        run,
                        manifest.references(),
                        Some((out, bytes)),
                        ExecutionOutcome::Completed,
                        None,
                        now,
                    )?;
                    admit_response(
                        ws, spec, store, run, node, &attempt, &response, &subject, now,
                    )
                }
                Err(failure) => {
                    let outcome = outcome_of(&failure);
                    let reason = format!("{failure}; stderr: {}", completion.stderr.trim());
                    let sealed = seal(
                        store,
                        &scheduler_for(ws, spec, run)?,
                        claim,
                        node,
                        run,
                        manifest.references(),
                        None,
                        outcome,
                        Some(reason.clone()),
                        now,
                    )?;
                    let disposition = retry(ws, store, spec, run, &claim.node_id, now)?;
                    Ok(json!({
                        "action": "attempt_failed",
                        "node": claim.node_id,
                        "attempt": sealed.number,
                        "reason": reason,
                        "disposition": disposition,
                    }))
                }
            }
        }
        other => Err(fail(format!(
            "this controller cannot execute a node producing {other}"
        ))),
    }
}

/// Verify a sealed completed attempt with the node's pinned verifier and
/// admit the receipt.
fn verify_and_admit(
    ws: &Workspace,
    spec: &GraphSpec,
    store: &mut Store,
    run: &crate::contracts::GraphRun,
    node: &NodeSpec,
    attempt: &Attempt,
    now: &Timestamp,
) -> Result<Value, RunError> {
    let subject = manifest_subject(&attempt.input_manifest)
        .ok_or_else(|| fail("the sealed attempt froze no code evidence"))?;
    let manifest_digest = run_try!(
        crate::digests::input_manifest_digest(
            &attempt.input_manifest,
            &attempt.dependency_receipts
        ),
        "manifest digest"
    );
    let claim_like = ClaimFields {
        node_id: node.id.clone(),
        number: attempt.number.get(),
    };
    let request = request_for_fields(
        node,
        run,
        &claim_like,
        &subject,
        &attempt.input_manifest,
        &manifest_digest,
    );
    let args = match node.acceptance.verifier.id.as_str() {
        patch::VERIFIER_ID => {
            let artifact = attempt
                .result
                .as_ref()
                .and_then(|r| r.artifact_ref.strip_prefix("file:"))
                .ok_or_else(|| fail("the sealed attempt has no file artifact"))?;
            vec![
                "patch-verifier".to_owned(),
                "--artifact".to_owned(),
                artifact.to_owned(),
            ]
        }
        software::VERIFIER_ID => {
            let profile_path = ws
                .software_profile
                .as_ref()
                .ok_or_else(|| fail("no --software-profile was given"))?;
            let out = artifact_path(ws, run, &node.id, attempt.number.get())?;
            software::child_args(profile_path, &out)
        }
        other => return Err(fail(format!("no verifier named {other} is built in"))),
    };
    let host = host_profile(ws, args)?;
    let completion = verifier_host::run(&host, &request, &Arc::new(AtomicBool::new(false)));
    match completion.outcome {
        Ok(response) => admit_response(
            ws, spec, store, run, node, attempt, &response, &subject, now,
        ),
        Err(failure) => Ok(json!({
            "action": "verification_failed",
            "node": node.id,
            "attempt": attempt.number,
            "reason": format!("{failure}; stderr: {}", completion.stderr.trim()),
            "note": "the attempt stays sealed; the next step retries verification",
        })),
    }
}

/// Issue and admit the receipt for a verdict, then apply the retry policy
/// on a rejection.
#[allow(clippy::too_many_arguments)]
fn admit_response(
    ws: &Workspace,
    spec: &GraphSpec,
    store: &mut Store,
    run: &crate::contracts::GraphRun,
    node: &NodeSpec,
    attempt: &Attempt,
    response: &crate::verifier_host::VerifierResponse,
    subject: &str,
    now: &Timestamp,
) -> Result<Value, RunError> {
    let receipt_id = run_try!(
        Ident::new(format!("rcpt-{}-{}", node.id, attempt.number.get())),
        "receipt id"
    );
    let receipt = run_try!(
        receipts::issue(
            response,
            node,
            attempt,
            receipt_id,
            subject.to_owned(),
            format!("checkspan-controller:{}", ws.controller),
            now.clone(),
            Validity {
                valid_until: None,
                conditions: vec![],
            },
        ),
        "cannot issue the receipt"
    );
    run_try!(
        receipts::admit(store, spec, run, &receipt, now),
        "cannot admit the receipt"
    );
    let mut report = json!({
        "action": "verified",
        "node": node.id,
        "attempt": attempt.number,
        "verdict": receipt.verdict.as_str(),
        "receipt": receipt.id,
    });
    if receipt.verdict == crate::contracts::Verdict::Reject {
        let disposition = retry(ws, store, spec, run, &node.id, now)?;
        report["disposition"] = disposition;
    }
    if receipt.verdict == crate::contracts::Verdict::Undecidable {
        let expires = run_try!(Timestamp::new(ws.gate_expiry.clone()), "gate expiry");
        let packet = run_try!(
            crate::gates::open_resolve_work(store, spec, run, &node.id, now, &expires),
            "cannot open the gate packet"
        );
        report["packet"] = json!(packet.id);
    }
    Ok(report)
}

// ------------------------------------------------------------- helpers --

fn load_graph(path: &Path) -> Result<GraphSpec, RunError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| fail(format!("cannot read {}: {e}", path.display())))?;
    let spec: GraphSpec = run_try!(parse_record(&text), "the graph document is invalid");
    run_try!(
        crate::graph::admit(&spec).map_err(|errors| errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")),
        "the graph is not admissible"
    );
    Ok(spec)
}

fn open_run(
    store_path: &Path,
    spec: &GraphSpec,
    run_id: &str,
) -> Result<(Store, crate::contracts::GraphRun), RunError> {
    let run_id = run_try!(RunId::new(run_id), "run id");
    let mut store = run_try!(Store::open(store_path), "cannot open the store");
    // Bind the graph file to the stored revision; an edited file conflicts.
    run_try!(
        store.store_graph(spec),
        "the graph file does not match the stored revision"
    );
    let run = run_try!(store.run(&run_id), "cannot read the run")
        .ok_or_else(|| fail(format!("run {run_id} does not exist in this store")))?;
    Ok((store, run))
}

fn scheduler_for<'a>(
    ws: &Workspace,
    spec: &'a GraphSpec,
    run: &'a crate::contracts::GraphRun,
) -> Result<Scheduler<'a>, RunError> {
    let controller = run_try!(ControllerId::new(&ws.controller), "controller id");
    Ok(run_try!(Scheduler::new(spec, run, controller), "scheduler"))
}

fn resolve_evidence(
    ws: &Workspace,
    spec: &GraphSpec,
    store: &Store,
    run: &crate::contracts::GraphRun,
    node: &NodeSpec,
    claim: &Claim,
    now: &Timestamp,
) -> Result<crate::evidence::Manifest, String> {
    let mut sources = Sources::default();
    if let Some(repo) = &ws.repo {
        sources
            .register(REPO_SOURCE, Source::Repository { root: repo.clone() })
            .expect("one registration");
    }
    let limits = Limits::default();
    let resolver = Resolver::new(&sources, &limits, now);
    let events = store.events(&run.run_id).map_err(|e| e.to_string())?;
    let state = RunState::replay(run.run_id.clone(), spec, &events).map_err(|e| e.to_string())?;
    let service = DependencyService::new(spec, run, &state, store, now);
    let snapshot = service.resolve(&node.id).map_err(|blockers| {
        blockers
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let offers: Vec<Offer> = node
        .evidence_ports
        .iter()
        .filter(|port| {
            port.kind == crate::contracts::PortKind::Code
                && !claim
                    .narrowing
                    .as_ref()
                    .is_some_and(|n| n.dropped_ports.contains(&port.name))
        })
        .map(|port| Offer {
            port: port.name.clone(),
            source: REPO_SOURCE.to_owned(),
            item: Item::Candidate(Selection {
                candidate: Selector::WorkingTree,
                base: None,
            }),
        })
        .collect();
    resolver
        .resolve(node, claim.narrowing.as_ref(), &snapshot, &offers)
        .map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        })
}

/// Seal the claimed attempt through the scheduler's fenced completion.
#[allow(clippy::too_many_arguments)]
fn seal(
    store: &mut Store,
    scheduler: &Scheduler<'_>,
    claim: &Claim,
    node: &NodeSpec,
    run: &crate::contracts::GraphRun,
    input_manifest: Vec<crate::contracts::EvidenceRef>,
    result: Option<(PathBuf, Vec<u8>)>,
    outcome: ExecutionOutcome,
    error_text: Option<String>,
    now: &Timestamp,
) -> Result<Attempt, RunError> {
    let attempt = Attempt {
        record: RecordKind::Attempt,
        schema_version: SchemaVersion(1),
        run_id: run.run_id.clone(),
        node: crate::contracts::NodeRef {
            graph_id: run.graph_ref.graph_id.clone(),
            node_id: claim.node_id.clone(),
            revision: node.revision,
        },
        number: claim.number,
        owner: claim.owner.as_str().to_owned(),
        started_at: now.clone(),
        finished_at: Some(now.clone()),
        repair_hint: claim.narrowing.as_ref().and_then(|n| n.repair_hint.clone()),
        input_manifest,
        dependency_receipts: claim.dependency_receipts.clone(),
        result: result
            .as_ref()
            .map(|(path, bytes)| crate::contracts::ResultRef {
                result_type: node.result_type.clone(),
                artifact_ref: format!("file:{}", path.display()),
                digest: crate::digests::artifact_digest(bytes),
            }),
        produced_evidence: Vec::new(),
        execution: Some(Execution {
            outcome,
            error_code: None,
            error_text,
        }),
        verifier_receipt: None,
    };
    run_try!(scheduler.complete(store, claim, &attempt), "cannot seal");
    Ok(attempt)
}

fn retry(
    ws: &Workspace,
    store: &mut Store,
    spec: &GraphSpec,
    run: &crate::contracts::GraphRun,
    node_id: &crate::contracts::NodeId,
    now: &Timestamp,
) -> Result<Value, RunError> {
    let narrowing = Narrowing {
        repair_hint: None,
        dropped_ports: vec![],
        restricted_sources: vec![],
    };
    let disposition = run_try!(
        budget::apply_retry_policy(store, spec, run, node_id, &narrowing, now),
        "retry policy"
    );
    Ok(match disposition {
        Disposition::Retry { class, .. } => json!({
            "decision": "retry",
            "class": class,
        }),
        Disposition::Exhausted { reason, route } => {
            let mut report = json!({
                "decision": "exhausted",
                "reason": reason.to_string(),
                "route": route,
            });
            if route == crate::contracts::OnExhaustion::Gate {
                let expires = run_try!(Timestamp::new(ws.gate_expiry.clone()), "gate expiry");
                let packet = run_try!(
                    crate::gates::open_resolve_work(store, spec, run, node_id, now, &expires),
                    "cannot open the gate packet"
                );
                report["packet"] = json!(packet.id);
            }
            report
        }
    })
}

struct ClaimFields {
    node_id: crate::contracts::NodeId,
    number: u32,
}

fn request_for(
    node: &NodeSpec,
    run: &crate::contracts::GraphRun,
    claim: &Claim,
    subject: &str,
    evidence: &[crate::contracts::EvidenceRef],
    manifest_digest: &crate::contracts::Digest,
) -> VerifierRequest {
    request_for_fields(
        node,
        run,
        &ClaimFields {
            node_id: claim.node_id.clone(),
            number: claim.number.get(),
        },
        subject,
        evidence,
        manifest_digest,
    )
}

fn request_for_fields(
    node: &NodeSpec,
    run: &crate::contracts::GraphRun,
    claim: &ClaimFields,
    subject: &str,
    evidence: &[crate::contracts::EvidenceRef],
    manifest_digest: &crate::contracts::Digest,
) -> VerifierRequest {
    VerifierRequest {
        protocol: crate::contracts::Exactly,
        verifier: node.acceptance.verifier.clone(),
        attempt: crate::contracts::AttemptRef {
            run_id: run.run_id.clone(),
            node: crate::contracts::NodeRef {
                graph_id: run.graph_ref.graph_id.clone(),
                node_id: claim.node_id.clone(),
                revision: node.revision,
            },
            number: crate::contracts::AttemptNumber::new(claim.number).expect("a claim number"),
        },
        subject: subject.to_owned(),
        claim: node.acceptance.claim.clone(),
        required_checks: node.acceptance.required_checks.clone(),
        policy_ref: node.acceptance.policy_ref.clone(),
        input_manifest_digest: manifest_digest.clone(),
        evidence: evidence.to_vec(),
    }
}

fn manifest_subject(evidence: &[crate::contracts::EvidenceRef]) -> Option<String> {
    evidence
        .iter()
        .find(|r| r.subject.starts_with("patch_subject:"))
        .map(|r| r.subject.clone())
}

fn artifact_path(
    ws: &Workspace,
    run: &crate::contracts::GraphRun,
    node_id: &crate::contracts::NodeId,
    number: u32,
) -> Result<PathBuf, RunError> {
    let dir = ws.artifacts.join(run.run_id.as_str());
    run_try!(
        std::fs::create_dir_all(&dir),
        "cannot create the artifact directory"
    );
    Ok(dir.join(format!("{node_id}-{number}.json")))
}

fn host_profile(ws: &Workspace, args: Vec<String>) -> Result<VerifierProfile, RunError> {
    let executable = run_try!(std::env::current_exe(), "cannot locate this binary");
    let mut env = vec![];
    if let Ok(path) = std::env::var("PATH") {
        env.push(("PATH".to_owned(), path));
    }
    Ok(VerifierProfile {
        executable,
        executable_digest: None,
        args,
        cwd: ws.artifacts.clone(),
        env,
        timeout: VERIFIER_TIMEOUT,
        max_stdout_bytes: VERIFIER_OUTPUT_CAP,
        max_stderr_bytes: VERIFIER_OUTPUT_CAP,
    })
}

fn outcome_of(failure: &ProcessFailure) -> ExecutionOutcome {
    match failure {
        ProcessFailure::TimedOut { .. } => ExecutionOutcome::TimedOut,
        ProcessFailure::Cancelled => ExecutionOutcome::Cancelled,
        _ => ExecutionOutcome::Failed,
    }
}
