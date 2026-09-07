//! Scoped local evidence resolved into the input manifest frozen at dispatch.
//!
//! Real repositories and directories on disk; every refusal is a named
//! error, and a frozen manifest is verified again against the same sources.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use checkspan::adapters::code::{Selection, Selector};
use checkspan::budget::Narrowing;
use checkspan::contracts::{
    Digest, GraphSpec, Ident, NodeId, NodeSpec, PatchResult, PortKind, ReceiptRef, Record, RunId,
    Timestamp, parse_record,
};
use checkspan::deps::{DependencySnapshot, Provenance, ResolvedDependency};
use checkspan::digests::{artifact_digest, input_manifest_digest, patch_subject_digest};
use checkspan::evidence::{
    Attestation, EvidenceError, Item, Limits, Offer, Resolver, Source, Sources,
};
use common::{fixture, schema_errors};

const NOW: &str = "2026-09-06T12:00:00Z";
const LOG: &[u8] = b"line one\n";

fn ts(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}

fn ident(s: &str) -> Ident {
    Ident::new(s).unwrap()
}

fn spec() -> GraphSpec {
    parse_record(&fixture("evidence/graph-with-ports.json")).unwrap()
}

fn node<'a>(spec: &'a GraphSpec, id: &str) -> &'a NodeSpec {
    spec.nodes.iter().find(|n| n.id.as_str() == id).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("checkspan-evidence-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Checkspan Fixture",
            "-c",
            "user.email=fixture@checkspan.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .expect("git is installed");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository with one commit and one uncommitted edit.
fn repo(name: &str) -> PathBuf {
    let root = temp_dir(&format!("{name}-repo"));
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "core.autocrlf", "false"]);
    fs::write(root.join("README.md"), b"# fixture\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        b"pub fn one() -> u32 {\n    1\n}\n",
    )
    .unwrap();
    root
}

/// A log directory with `ci/build.log` and a manifest that vouches for it;
/// returns the directory and the manifest's digest.
fn logs(name: &str) -> (PathBuf, Digest) {
    let root = temp_dir(&format!("{name}-logs"));
    fs::create_dir_all(root.join("ci")).unwrap();
    fs::write(root.join("ci/build.log"), LOG).unwrap();
    let manifest = format!("ci/build.log {}\n", artifact_digest(LOG));
    fs::write(root.join("MANIFEST.txt"), manifest.as_bytes()).unwrap();
    (root, artifact_digest(manifest.as_bytes()))
}

fn directory(root: &Path, attestation: Option<Attestation>, valid_until: Option<&str>) -> Source {
    Source::Directory {
        root: root.to_path_buf(),
        producer: "local-ci".into(),
        attestation,
        valid_until: valid_until.map(ts),
    }
}

fn sources(repo: &Path, logs: &Path, attestation: Digest) -> Sources {
    let mut sources = Sources::default();
    sources
        .register(
            "repo:pilot",
            Source::Repository {
                root: repo.to_path_buf(),
            },
        )
        .unwrap();
    sources
        .register(
            "logs:ci",
            directory(
                logs,
                Some(Attestation {
                    reference: "MANIFEST.txt".into(),
                    digest: attestation,
                }),
                Some("2026-12-31T00:00:00Z"),
            ),
        )
        .unwrap();
    sources
}

fn empty_snapshot(node: &str) -> DependencySnapshot {
    DependencySnapshot {
        node: NodeId::new(node).unwrap(),
        resolved: vec![],
    }
}

/// cs_ci's dependency on cs_patch, pinned to receipt `rcpt-patch-1`.
fn patch_snapshot(spec: &GraphSpec, result_digest: Digest) -> DependencySnapshot {
    DependencySnapshot {
        node: NodeId::new("cs_ci").unwrap(),
        resolved: vec![ResolvedDependency {
            dependency: node(spec, "cs_ci").deps[0].clone(),
            receipt: ReceiptRef {
                id: ident("rcpt-patch-1"),
                run_id: RunId::new("run-0001").unwrap(),
                node: spec.node_ref(&NodeId::new("cs_patch").unwrap()).unwrap(),
            },
            provenance: Provenance::SameRun,
            result_digest,
        }],
    }
}

fn candidate(port: &str) -> Offer {
    Offer {
        port: ident(port),
        source: "repo:pilot".into(),
        item: Item::Candidate(Selection {
            candidate: Selector::WorkingTree,
            base: None,
        }),
    }
}

fn file(port: &str, path: &str, expected_digest: Option<Digest>) -> Offer {
    Offer {
        port: ident(port),
        source: "logs:ci".into(),
        item: Item::File {
            path: path.into(),
            subject: "commit:base".into(),
            version: "run-7".into(),
            expected_digest,
        },
    }
}

fn ports(manifest: &checkspan::evidence::Manifest) -> Vec<&str> {
    manifest
        .evidence
        .iter()
        .map(|r| r.reference.port_name.as_str())
        .collect()
}

fn names(idents: &[Ident]) -> Vec<&str> {
    idents.iter().map(Ident::as_str).collect()
}

#[test]
fn code_and_log_ports_resolve_into_a_frozen_manifest_that_verifies_until_something_changes() {
    let spec = spec();
    let repo = repo("resolve");
    let (log_dir, attestation) = logs("resolve");
    let sources = sources(&repo, &log_dir, attestation);
    let limits = Limits::default();
    let now = ts(NOW);
    let resolver = Resolver::new(&sources, &limits, &now);
    let patch = node(&spec, "cs_patch");
    let deps = empty_snapshot("cs_patch");
    let offers = [
        candidate("candidate"),
        file("build_log", "ci/build.log", None),
    ];

    let manifest = resolver.resolve(patch, None, &deps, &offers).unwrap();
    assert_eq!(ports(&manifest), ["build_log", "candidate"]);
    assert_eq!(names(&manifest.omitted), ["research"]);

    let code = &manifest.evidence[1];
    let bytes = code.bytes.as_ref().unwrap();
    let text = std::str::from_utf8(bytes).unwrap();
    assert_eq!(
        schema_errors(PatchResult::SCHEMA_ID, text),
        Vec::<String>::new()
    );
    let record: PatchResult = parse_record(text).unwrap();
    assert_eq!(
        code.reference.subject,
        format!("patch_subject:{}", patch_subject_digest(&record).unwrap())
    );
    assert_eq!(code.reference.content_digest, artifact_digest(bytes));
    assert!(
        code.reference.locator.starts_with("git:"),
        "{}",
        code.reference.locator
    );
    assert!(
        code.reference.locator.contains("&candidate=working_tree:"),
        "{}",
        code.reference.locator
    );
    assert_eq!(code.reference.version, record.candidate.commit.to_string());
    assert_eq!(code.reference.provenance.producer, "checkspan/code-adapter");
    assert_eq!(code.reference.provenance.attestation_ref, None);
    assert_eq!(
        code.reference.policy_ref,
        patch.evidence_ports[0].handling_policy_ref
    );
    assert_eq!(code.reference.valid_until, None);

    let log = &manifest.evidence[0];
    assert_eq!(log.reference.locator, "file:logs:ci/ci/build.log");
    assert_eq!(log.reference.subject, "commit:base");
    assert_eq!(log.reference.version, "run-7");
    assert_eq!(log.reference.content_digest, artifact_digest(LOG));
    assert_eq!(log.bytes.as_deref(), Some(LOG));
    assert_eq!(log.reference.provenance.producer, "local-ci");
    assert_eq!(
        log.reference.provenance.attestation_ref.as_deref(),
        Some("file:logs:ci/MANIFEST.txt")
    );
    assert_eq!(log.reference.valid_until, Some(ts("2026-12-31T00:00:00Z")));
    assert_eq!(
        log.reference.policy_ref,
        patch.evidence_ports[1].handling_policy_ref
    );

    assert_eq!(
        manifest.digest,
        input_manifest_digest(&manifest.references(), &[]).unwrap()
    );
    assert!(
        resolver
            .verify(patch, &manifest.references(), &deps)
            .is_empty()
    );
    let again = resolver.resolve(patch, None, &deps, &offers).unwrap();
    assert_eq!(again.digest, manifest.digest);

    // The candidate and the log change underneath the frozen manifest.
    fs::write(
        repo.join("src/lib.rs"),
        b"pub fn one() -> u32 {\n    2\n}\n",
    )
    .unwrap();
    fs::write(log_dir.join("ci/build.log"), b"line one\nline two\n").unwrap();
    let errors = resolver.verify(patch, &manifest.references(), &deps);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::DigestMismatch { port, .. } if port.as_str() == "build_log"),
        "{}",
        errors[0]
    );
    assert!(
        matches!(&errors[1], EvidenceError::SubjectChanged { port, expected, actual }
            if port.as_str() == "candidate" && expected != actual),
        "{}",
        errors[1]
    );
    let changed = resolver.resolve(patch, None, &deps, &offers).unwrap();
    assert_ne!(changed.digest, manifest.digest);
}

#[test]
fn proofs_come_from_pinned_receipts_and_missing_required_evidence_blocks_dispatch() {
    let spec = spec();
    let repo = repo("proofs");
    let (log_dir, attestation) = logs("proofs");
    let sources = sources(&repo, &log_dir, attestation);
    let limits = Limits::default();
    let now = ts(NOW);
    let resolver = Resolver::new(&sources, &limits, &now);
    let ci = node(&spec, "cs_ci");
    let result_digest = artifact_digest(b"the exact patch bytes");
    let deps = patch_snapshot(&spec, result_digest.clone());
    let offers = [candidate("checkout"), file("ci_log", "ci/build.log", None)];

    let manifest = resolver.resolve(ci, None, &deps, &offers).unwrap();
    assert_eq!(ports(&manifest), ["checkout", "ci_log", "patch"]);
    assert_eq!(names(&manifest.omitted), ["review"]);
    let proof = &manifest.evidence[2];
    assert_eq!(proof.reference.locator, "receipt:rcpt-patch-1");
    assert_eq!(
        proof.reference.subject,
        format!("node:{}", deps.resolved[0].receipt.node)
    );
    assert_eq!(proof.reference.version, "rcpt-patch-1");
    assert_eq!(proof.reference.content_digest, result_digest);
    assert_eq!(proof.reference.provenance.producer, "run:run-0001");
    assert_eq!(
        proof.reference.provenance.attestation_ref.as_deref(),
        Some("receipt:rcpt-patch-1")
    );
    assert_eq!(proof.reference.valid_until, None);
    assert_eq!(proof.bytes, None);
    assert_eq!(
        manifest.digest,
        input_manifest_digest(&manifest.references(), &deps.receipt_refs()).unwrap()
    );
    assert!(
        resolver
            .verify(ci, &manifest.references(), &deps)
            .is_empty()
    );

    let errors = resolver
        .resolve(ci, None, &deps, &[candidate("checkout")])
        .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::MissingRequired(port) if port.as_str() == "ci_log"),
        "{}",
        errors[0]
    );

    let errors = resolver
        .resolve(ci, None, &empty_snapshot("cs_ci"), &offers)
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, EvidenceError::ProofMissing { port, scope }
            if port.as_str() == "patch" && scope == &["node:cs_patch".to_string()])),
        "{errors:?}"
    );

    let mut offered_proof = offers.to_vec();
    offered_proof.push(file("patch", "ci/build.log", None));
    let errors = resolver
        .resolve(ci, None, &deps, &offered_proof)
        .unwrap_err();
    assert!(
        errors.iter().any(
            |e| matches!(e, EvidenceError::ProofsNotOffered(port) if port.as_str() == "patch")
        ),
        "{errors:?}"
    );

    let other = patch_snapshot(&spec, artifact_digest(b"another result"));
    let errors = resolver.verify(ci, &manifest.references(), &other);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::DigestMismatch { port, .. } if port.as_str() == "patch"),
        "{}",
        errors[0]
    );
    let errors = resolver.verify(ci, &manifest.references(), &empty_snapshot("cs_ci"));
    assert!(
        matches!(&errors[0], EvidenceError::ProofMissing { port, .. } if port.as_str() == "patch"),
        "{}",
        errors[0]
    );
}

#[test]
fn undeclared_unregistered_wrong_kind_and_wrongly_typed_sources_are_refused() {
    let spec = spec();
    let repo = repo("scope");
    let (log_dir, attestation) = logs("scope");
    let limits = Limits::default();
    let now = ts(NOW);
    let patch = node(&spec, "cs_patch");
    let deps = empty_snapshot("cs_patch");
    let full = sources(&repo, &log_dir, attestation.clone());
    let resolver = Resolver::new(&full, &limits, &now);

    let outside = Offer {
        source: "repo:other".into(),
        ..candidate("candidate")
    };
    let errors = resolver
        .resolve(patch, None, &deps, &[outside])
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::UndeclaredSource { port, source }
            if port.as_str() == "candidate" && source == "repo:other"),
        "{}",
        errors[0]
    );

    let mut only_repo = Sources::default();
    only_repo
        .register("repo:pilot", Source::Repository { root: repo.clone() })
        .unwrap();
    let thin = Resolver::new(&only_repo, &limits, &now);
    let errors = thin
        .resolve(
            patch,
            None,
            &deps,
            &[
                candidate("candidate"),
                file("build_log", "ci/build.log", None),
            ],
        )
        .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::UnregisteredSource { port, source }
            if port.as_str() == "build_log" && source == "logs:ci"),
        "{}",
        errors[0]
    );

    let mut swapped = Sources::default();
    swapped
        .register("repo:pilot", directory(&log_dir, None, None))
        .unwrap();
    let wrong = Resolver::new(&swapped, &limits, &now);
    let errors = wrong
        .resolve(patch, None, &deps, &[candidate("candidate")])
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::SourceKindMismatch { port, source, kind: PortKind::Code }
            if port.as_str() == "candidate" && source == "repo:pilot"),
        "{}",
        errors[0]
    );

    let wrong_item = Offer {
        source: "repo:pilot".into(),
        ..file("candidate", "ci/build.log", None)
    };
    let errors = resolver
        .resolve(patch, None, &deps, &[wrong_item])
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::ItemKindMismatch(port) if port.as_str() == "candidate"),
        "{}",
        errors[0]
    );

    let errors = resolver
        .resolve(
            patch,
            None,
            &deps,
            &[
                candidate("candidate"),
                candidate("candidate"),
                candidate("nonexistent"),
            ],
        )
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, EvidenceError::DuplicateOffer(p) if p.as_str() == "candidate")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, EvidenceError::UnknownPort(p) if p.as_str() == "nonexistent")),
        "{errors:?}"
    );

    // A retry narrowing that dropped the optional log port refuses an offer
    // for it and resolves without it.
    let narrowing = Narrowing {
        repair_hint: None,
        dropped_ports: vec![ident("build_log")],
        restricted_sources: vec![],
    };
    let errors = resolver
        .resolve(
            patch,
            Some(&narrowing),
            &deps,
            &[
                candidate("candidate"),
                file("build_log", "ci/build.log", None),
            ],
        )
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::DroppedPort(p) if p.as_str() == "build_log"),
        "{}",
        errors[0]
    );
    let narrowed = resolver
        .resolve(patch, Some(&narrowing), &deps, &[candidate("candidate")])
        .unwrap();
    assert_eq!(ports(&narrowed), ["candidate"]);
    assert_eq!(names(&narrowed.omitted), ["research"]);

    // A code port declared against a type this build does not produce.
    let legacy = node(&spec, "cs_legacy");
    let errors = resolver
        .resolve(
            legacy,
            None,
            &empty_snapshot("cs_legacy"),
            &[candidate("candidate")],
        )
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::TypeMismatch { port, expected, .. }
            if port.as_str() == "candidate" && expected.schema_id.as_str() == "code_ref"),
        "{}",
        errors[0]
    );
}

#[test]
fn paths_cannot_escape_their_source_and_sizes_are_bounded() {
    let spec = spec();
    let repo = repo("paths");
    let (log_dir, attestation) = logs("paths");
    let sources = sources(&repo, &log_dir, attestation);
    let now = ts(NOW);
    let patch = node(&spec, "cs_patch");
    let deps = empty_snapshot("cs_patch");
    let limits = Limits::default();
    let resolver = Resolver::new(&sources, &limits, &now);

    let refused = |path: &str| {
        let errors = resolver
            .resolve(
                patch,
                None,
                &deps,
                &[candidate("candidate"), file("build_log", path, None)],
            )
            .unwrap_err();
        assert_eq!(errors.len(), 1, "{path}: {errors:?}");
        errors.into_iter().next().unwrap()
    };
    assert!(matches!(
        refused("../MANIFEST.txt"),
        EvidenceError::PathNotRelative { .. }
    ));
    assert!(matches!(
        refused("/ci/build.log"),
        EvidenceError::PathNotRelative { .. }
    ));
    assert!(matches!(
        refused("ci/./build.log"),
        EvidenceError::PathNotRelative { .. }
    ));
    assert!(matches!(refused("ci"), EvidenceError::NotAFile { .. }));
    assert!(matches!(
        refused("ci/missing.log"),
        EvidenceError::Io { .. }
    ));

    // A link that leads outside the directory is refused once resolved.
    let outside = temp_dir("paths-outside");
    fs::write(outside.join("secret.log"), b"not evidence\n").unwrap();
    let link = log_dir.join("ci/link.log");
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(outside.join("secret.log"), &link);
    #[cfg(not(windows))]
    let linked = std::os::unix::fs::symlink(outside.join("secret.log"), &link);
    match linked {
        Ok(()) => {
            assert!(matches!(
                refused("ci/link.log"),
                EvidenceError::PathEscapes { .. }
            ));
        }
        Err(e) => eprintln!("symlink escape not exercised: cannot create a symlink here: {e}"),
    }

    let per_file = Limits {
        max_file_bytes: 4,
        ..Limits::default()
    };
    let errors = Resolver::new(&sources, &per_file, &now)
        .resolve(
            patch,
            None,
            &deps,
            &[
                candidate("candidate"),
                file("build_log", "ci/build.log", None),
            ],
        )
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::TooLarge { port, size: 9, max: 4, .. } if port.as_str() == "build_log"),
        "{}",
        errors[0]
    );

    let total = Limits {
        max_total_bytes: 4,
        ..Limits::default()
    };
    let errors = Resolver::new(&sources, &total, &now)
        .resolve(
            patch,
            None,
            &deps,
            &[
                candidate("candidate"),
                file("build_log", "ci/build.log", None),
            ],
        )
        .unwrap_err();
    assert!(
        errors
            .iter()
            .all(|e| matches!(e, EvidenceError::TotalTooLarge { max: 4, .. })),
        "{errors:?}"
    );
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn attestations_expected_digests_and_validity_are_enforced() {
    let spec = spec();
    let repo = repo("attest");
    let (log_dir, attestation) = logs("attest");
    let limits = Limits::default();
    let now = ts(NOW);
    let patch = node(&spec, "cs_patch");
    let deps = empty_snapshot("cs_patch");
    let offers = [
        candidate("candidate"),
        file("build_log", "ci/build.log", None),
    ];

    let with = |attestation: Option<Attestation>, valid_until: Option<&str>| {
        let mut sources = Sources::default();
        sources
            .register("repo:pilot", Source::Repository { root: repo.clone() })
            .unwrap();
        sources
            .register("logs:ci", directory(&log_dir, attestation, valid_until))
            .unwrap();
        sources
    };

    let wrong = with(
        Some(Attestation {
            reference: "MANIFEST.txt".into(),
            digest: artifact_digest(b"something else"),
        }),
        None,
    );
    let errors = Resolver::new(&wrong, &limits, &now)
        .resolve(patch, None, &deps, &offers)
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::InvalidAttestation { source, reason }
            if source == "logs:ci" && reason.contains("has digest")),
        "{}",
        errors[0]
    );

    let missing = with(
        Some(Attestation {
            reference: "ABSENT.txt".into(),
            digest: attestation.clone(),
        }),
        None,
    );
    let errors = Resolver::new(&missing, &limits, &now)
        .resolve(patch, None, &deps, &offers)
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::InvalidAttestation { .. }),
        "{}",
        errors[0]
    );

    let good = with(
        Some(Attestation {
            reference: "MANIFEST.txt".into(),
            digest: attestation.clone(),
        }),
        Some("2026-12-31T00:00:00Z"),
    );
    let resolver = Resolver::new(&good, &limits, &now);
    let errors = resolver
        .resolve(
            patch,
            None,
            &deps,
            &[
                candidate("candidate"),
                file(
                    "build_log",
                    "ci/build.log",
                    Some(artifact_digest(b"expected other bytes")),
                ),
            ],
        )
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::DigestMismatch { port, expected, actual }
            if port.as_str() == "build_log" && *actual == artifact_digest(LOG) && expected != actual),
        "{}",
        errors[0]
    );
    let manifest = resolver
        .resolve(
            patch,
            None,
            &deps,
            &[
                candidate("candidate"),
                file("build_log", "ci/build.log", Some(artifact_digest(LOG))),
            ],
        )
        .unwrap();
    assert_eq!(ports(&manifest), ["build_log", "candidate"]);

    // A source past its validity cannot feed new work, and a frozen manifest
    // whose evidence expired no longer verifies.
    let stale = with(None, Some("2026-01-01T00:00:00Z"));
    let errors = Resolver::new(&stale, &limits, &now)
        .resolve(patch, None, &deps, &offers)
        .unwrap_err();
    assert!(
        matches!(&errors[0], EvidenceError::Expired { port, valid_until }
            if port.as_str() == "build_log" && valid_until == &ts("2026-01-01T00:00:00Z")),
        "{}",
        errors[0]
    );
    let later = ts("2027-01-01T00:00:00Z");
    let errors = Resolver::new(&good, &limits, &later).verify(patch, &manifest.references(), &deps);
    assert!(
        errors.iter().any(
            |e| matches!(e, EvidenceError::Expired { port, .. } if port.as_str() == "build_log")
        ),
        "{errors:?}"
    );
}

#[test]
fn retrieval_and_human_ports_are_refused_rather_than_faked() {
    let spec = spec();
    let repo = repo("unsupported");
    let (log_dir, attestation) = logs("unsupported");
    let mut sources = sources(&repo, &log_dir, attestation);
    sources
        .register("corpus:none", directory(&log_dir, None, None))
        .unwrap();
    let limits = Limits::default();
    let now = ts(NOW);
    let resolver = Resolver::new(&sources, &limits, &now);

    let research = node(&spec, "cs_research");
    let errors = resolver
        .resolve(research, None, &empty_snapshot("cs_research"), &[])
        .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::Unsupported { port, kind: PortKind::Rag } if port.as_str() == "corpus"),
        "{}",
        errors[0]
    );

    let patch = node(&spec, "cs_patch");
    let offered = Offer {
        source: "corpus:none".into(),
        ..file("research", "ci/build.log", None)
    };
    let errors = resolver
        .resolve(
            patch,
            None,
            &empty_snapshot("cs_patch"),
            &[candidate("candidate"), offered],
        )
        .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::Unsupported { port, kind: PortKind::Rag } if port.as_str() == "research"),
        "{}",
        errors[0]
    );

    let ci = node(&spec, "cs_ci");
    let deps = patch_snapshot(&spec, artifact_digest(b"result"));
    let review = Offer {
        source: "gate:cs_ci".into(),
        ..file("review", "ci/build.log", None)
    };
    let errors = resolver
        .resolve(
            ci,
            None,
            &deps,
            &[
                candidate("checkout"),
                file("ci_log", "ci/build.log", None),
                review,
            ],
        )
        .unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors[0], EvidenceError::Unsupported { port, kind: PortKind::Human } if port.as_str() == "review"),
        "{}",
        errors[0]
    );
}
