//! Command-line entry point: `validate` and `inspect` over one document.
//!
//! Both commands read one file, run the offline validation pipeline (and
//! graph admission for a graph), and print one JSON object to stdout. Neither
//! dispatches work, creates a run store, or writes anywhere.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use crate::contracts::RecordKind;
use crate::graph::{self, AdmittedGraph};
use crate::validation::{self, Diagnostic, Limits, Report, Stage, ValidatedRecord};

/// Exit code when the document is rejected.
pub const EXIT_INVALID: u8 = 1;
/// Exit code for a usage error (clap's convention).
pub const EXIT_USAGE: u8 = 2;
/// Exit code when the document cannot be read.
pub const EXIT_IO: u8 = 3;

/// Top-level command line.
#[derive(Debug, Parser)]
#[command(
    name = "checkspan",
    version,
    about = "Work you can verify.",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Read-only commands over one record document.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate one record document offline and print every diagnostic as
    /// JSON. Exit 0 when valid, 1 when rejected, 3 when the file cannot be
    /// read.
    Validate {
        /// Path to a JSON record document.
        file: PathBuf,
    },
    /// Validate one record document and describe it as JSON. For a graph:
    /// nodes, targets, dependency order, required closure, and gates. Never
    /// dispatches work or creates a run store.
    Inspect {
        /// Path to a JSON record document.
        file: PathBuf,
    },
    /// Run the built-in software-check verifier as a protocol child: one
    /// request on standard input, one response on standard output, and the
    /// typed result written to --out. Meant to be spawned by a controller
    /// under a pinned profile.
    SoftwareVerifier {
        /// The pinned validation profile document.
        #[arg(long)]
        profile: PathBuf,
        /// Where the software check result record is written.
        #[arg(long)]
        out: PathBuf,
    },
}

/// Parse `args`, run the command, print its JSON to stdout, and return the
/// exit code. Exit codes: `0` valid, `1` rejected, `2` usage error, `3` the
/// file could not be read.
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => {
            if let Command::SoftwareVerifier { profile, out } = &cli.command {
                let code = crate::verifiers::software::run_child(profile, out);
                return ExitCode::from(code as u8);
            }
            let (code, output) = execute(&cli.command);
            let mut stdout = std::io::stdout().lock();
            let rendered = serde_json::to_string_pretty(&output).expect("output is JSON");
            if writeln!(stdout, "{rendered}").is_err() {
                return ExitCode::from(EXIT_IO);
            }
            ExitCode::from(code)
        }
        Err(err) => err.exit(),
    }
}

/// Run one command, returning its exit code and the JSON document to print.
pub fn execute(command: &Command) -> (u8, Value) {
    match command {
        Command::Validate { file } => match load(file) {
            Ok(report) => {
                let code = if report.is_valid() { 0 } else { EXIT_INVALID };
                (code, report_json("validate", file, &report))
            }
            Err(message) => (EXIT_IO, io_error_json("validate", file, &message)),
        },
        Command::SoftwareVerifier { .. } => (
            EXIT_USAGE,
            json!({
                "command": "software-verifier",
                "diagnostics": [{
                    "stage": "usage",
                    "path": "",
                    "message": "the software verifier runs as a protocol child; invoke the checkspan binary directly",
                }],
            }),
        ),
        Command::Inspect { file } => match load(file) {
            Ok(report) if report.is_valid() => {
                let record = report
                    .record
                    .as_ref()
                    .expect("valid reports carry a record");
                let mut output = report_json("inspect", file, &report);
                let (key, detail) = describe(record);
                output[key] = detail;
                (0, output)
            }
            Ok(report) => (EXIT_INVALID, report_json("inspect", file, &report)),
            Err(message) => (EXIT_IO, io_error_json("inspect", file, &message)),
        },
    }
}

/// Read and validate one file. A graph document also passes admission.
fn load(path: &Path) -> Result<Report, String> {
    let limits = Limits::default();
    let length = fs::metadata(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .len();
    if length > limits.max_bytes as u64 {
        return Ok(Report {
            header: None,
            diagnostics: vec![Diagnostic {
                stage: Stage::Size,
                path: String::new(),
                message: format!(
                    "document is {length} bytes; the limit is {}",
                    limits.max_bytes
                ),
            }],
            record: None,
        });
    }
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut report = validation::validate(&bytes, &limits);
    if let Some(ValidatedRecord::GraphSpec(spec)) = &report.record {
        let diagnostics = graph::admission_diagnostics(spec);
        if !diagnostics.is_empty() {
            report.diagnostics = diagnostics;
            report.record = None;
        }
    }
    Ok(report)
}

fn report_json(command: &str, path: &Path, report: &Report) -> Value {
    let kind = report.record.as_ref().map(ValidatedRecord::kind);
    json!({
        "command": command,
        "path": path.to_string_lossy(),
        "valid": report.is_valid(),
        "record": report.header.as_ref().map(|h| h.record.clone()),
        "schema_version": report.header.as_ref().map(|h| h.schema_version.0),
        "schema_id": kind.map(validation::schema_id_for),
        "diagnostics": report.diagnostics.iter().map(|d| json!({
            "stage": d.stage.as_str(),
            "path": d.path,
            "message": d.message,
        })).collect::<Vec<_>>(),
    })
}

fn io_error_json(command: &str, path: &Path, message: &str) -> Value {
    json!({
        "command": command,
        "path": path.to_string_lossy(),
        "valid": false,
        "record": Value::Null,
        "schema_version": Value::Null,
        "schema_id": Value::Null,
        "diagnostics": [{ "stage": "io", "path": "", "message": message }],
    })
}

/// The kind-specific description of a valid record: `graph` for a graph
/// spec, `identity` for everything else.
fn describe(record: &ValidatedRecord) -> (&'static str, Value) {
    match record {
        ValidatedRecord::GraphSpec(spec) => {
            let admitted = graph::admit(spec).expect("a valid report passed admission");
            ("graph", graph_json(&admitted))
        }
        ValidatedRecord::GraphRun(run) => (
            "identity",
            json!({
                "run_id": run.run_id,
                "graph_ref": run.graph_ref,
                "budget_lineage_ref": run.budget_lineage_ref,
            }),
        ),
        ValidatedRecord::Attempt(attempt) => (
            "identity",
            json!({
                "run_id": attempt.run_id,
                "node": attempt.node.to_string(),
                "number": attempt.number,
                "execution": attempt.execution.as_ref().map(|e| e.outcome.as_str()),
                "verifier_receipt": attempt.verifier_receipt.as_ref().map(|r| r.id.clone()),
            }),
        ),
        ValidatedRecord::VerifierReceipt(receipt) => (
            "identity",
            json!({
                "id": receipt.id,
                "attempt": receipt.attempt.to_string(),
                "verdict": receipt.verdict.as_str(),
                "provenance_level": receipt.provenance.level,
            }),
        ),
        ValidatedRecord::GatePacket(packet) => (
            "identity",
            json!({
                "id": packet.id,
                "run_id": packet.run_id,
                "node": packet.node.to_string(),
                "purpose": packet.purpose.as_str(),
                "options": packet.requested_decision.options,
            }),
        ),
        ValidatedRecord::GateDecision(decision) => (
            "identity",
            json!({
                "gate_packet_id": decision.gate_packet_id,
                "decision": decision.decision.as_str(),
                "authority": decision.authority.identity,
                "authority_level": decision.authority.level,
            }),
        ),
        ValidatedRecord::PatchResult(patch) => (
            "identity",
            json!({
                "base_commit": patch.base_commit,
                "candidate": patch.candidate.kind.as_str(),
                "commit": patch.candidate.commit,
                "changes": patch.changes.len(),
                "subject_digest": crate::digests::patch_subject_digest(patch)
                    .expect("a valid patch result has a subject digest"),
            }),
        ),
        ValidatedRecord::SoftwareCheckResult(result) => (
            "identity",
            json!({
                "attempt": result.attempt.to_string(),
                "subject": result.subject,
                "profile": format!("{}@{}", result.profile.id, result.profile.version),
                "conclusion": result.conclusion.as_str(),
                "checks": result.checks.len(),
            }),
        ),
        ValidatedRecord::NodeView(view) => (
            "identity",
            json!({
                "run_id": view.run_id,
                "node": view.node.to_string(),
                "status": view.status.as_str(),
                "current_attempt": view.current_attempt,
                "receipt": view.receipt.as_ref().map(|r| r.id.clone()),
            }),
        ),
    }
}

fn graph_json(graph: &AdmittedGraph) -> Value {
    let spec = &graph.spec;
    json!({
        "graph_id": spec.graph_id,
        "revision": spec.revision,
        "display_name": spec.display_name,
        "budget": {
            "total_attempts": spec.budget.total_attempts,
            "deadline": spec.budget.deadline,
        },
        "supersedes": spec.supersedes,
        "nodes": spec.nodes.iter().map(|n| json!({
            "id": n.id,
            "revision": n.revision,
            "kind": n.kind.as_str(),
            "display_name": n.display_name,
            "result_type": n.result_type.to_string(),
            "deps": n.deps.iter().map(|d| json!({
                "node_id": d.node.node_id,
                "revision": d.node.revision,
                "output_port": d.output_port,
                "expected_type": d.expected_type.to_string(),
            })).collect::<Vec<_>>(),
            "evidence_ports": n.evidence_ports.iter().map(|p| json!({
                "name": p.name,
                "kind": p.kind,
                "required": p.required,
                "allowed_source_scope": p.allowed_source_scope,
            })).collect::<Vec<_>>(),
            "required_checks": n.acceptance.required_checks,
            "verifier": format!("{}@{}", n.acceptance.verifier.id, n.acceptance.verifier.version),
            "max_attempts": n.retry_policy.max_attempts,
        })).collect::<Vec<_>>(),
        "targets": spec.targets.iter().map(|t| t.node_id.clone()).collect::<Vec<_>>(),
        "order": graph.order,
        "required": graph.required,
        "optional": graph.optional,
        "gates": graph.gates.iter().map(|g| json!({
            "gate": g.gate,
            "resolves": g.resolves,
        })).collect::<Vec<_>>(),
    })
}

/// The record kind named by a validated report, for callers that only have
/// the JSON.
pub fn record_kind_of(report: &Report) -> Option<RecordKind> {
    report.record.as_ref().map(ValidatedRecord::kind)
}
