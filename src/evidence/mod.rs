//! Resolution of scoped local evidence into the input manifest frozen at
//! dispatch.
//!
//! Every evidence port of a node is filled, or refused, before a worker or
//! verifier runs. Resolution reads only sources the run has registered under
//! the names a port's allowed scope lists; the declaration alone enforces
//! nothing, this module does. `code` ports resolve to a captured patch
//! subject, `logs` ports to bounded regular files inside a registered
//! directory, and `proofs` ports to the accepted receipt pinned for the
//! dependency. Retrieval (`rag`) and `human` ports are typed boundaries this
//! build cannot fill: an offer for them, or a requirement for them, is
//! refused rather than faked.
//!
//! A manifest can be verified again later against the same sources: a
//! candidate whose subject changed, a file whose bytes changed, a source past
//! its validity, or a receipt no longer pinned is reported before dispatch.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::adapters::code::{self, CaptureError, Selection, Selector};
use crate::budget::{Narrowing, NarrowingError, check_narrowing};
use crate::contracts::{
    CandidateKind, CommitId, Digest, EvidenceRef, Ident, NodeSpec, PatchResult, PortKind, PortSpec,
    Provenance, Record, Timestamp, TypeRef, is_clean_relative_path, registry,
};
use crate::deps::{DependencySnapshot, Provenance as DependencyProvenance};
use crate::digests::{CanonicalError, artifact_digest, input_manifest_digest, jcs_bytes};

/// Ceilings on what one resolution reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum bytes of one log file.
    pub max_file_bytes: u64,
    /// Maximum bytes read across every port.
    pub max_total_bytes: u64,
    /// Ceilings for capturing a code candidate.
    pub capture: code::Limits,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            capture: code::Limits::default(),
        }
    }
}

/// A document inside a directory source that vouches for the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    /// Path of the document relative to the directory.
    pub reference: String,
    /// Digest its bytes must have.
    pub digest: Digest,
}

/// A registered source a run may draw evidence from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A Git repository, for `code` ports.
    Repository {
        /// Any path inside the repository.
        root: PathBuf,
    },
    /// A directory of files, for `logs` ports.
    Directory {
        /// The directory; nothing outside it is readable.
        root: PathBuf,
        /// Who produced the files, as declared by the operator.
        producer: String,
        /// A document that vouches for the directory, when one exists.
        attestation: Option<Attestation>,
        /// Moment after which evidence from this source is stale.
        valid_until: Option<Timestamp>,
    },
}

/// The sources registered for a run, by the names port scopes use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sources {
    entries: BTreeMap<String, Source>,
}

impl Sources {
    /// Register a source under a name; a repeated name is refused.
    pub fn register(&mut self, name: impl Into<String>, source: Source) -> Result<(), String> {
        let name = name.into();
        if self.entries.contains_key(&name) {
            return Err(format!("source {name:?} is already registered"));
        }
        self.entries.insert(name, source);
        Ok(())
    }

    /// The source registered under `name`.
    pub fn get(&self, name: &str) -> Option<&Source> {
        self.entries.get(name)
    }

    /// Every registered name.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

/// What is offered for one port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// For `code` ports: which candidate of the repository.
    Candidate(Selection),
    /// For `logs` ports: one file.
    File {
        /// Path relative to the directory source.
        path: String,
        /// What the file is about: a commit, a run.
        subject: String,
        /// Producer-side version of the file.
        version: String,
        /// Digest the file must have, when the operator knows it.
        expected_digest: Option<Digest>,
    },
}

/// An operator's offer for one port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The port.
    pub port: Ident,
    /// The registered source name, which must be in the port's scope.
    pub source: String,
    /// What is offered.
    pub item: Item,
}

/// One resolved port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The reference frozen into the manifest.
    pub reference: EvidenceRef,
    /// The bytes the reference identifies; absent for a proof, whose bytes
    /// are the dependency's stored result.
    pub bytes: Option<Vec<u8>>,
}

/// The input manifest frozen at dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Resolved evidence, ordered by port name.
    pub evidence: Vec<Resolved>,
    /// Optional ports left without evidence.
    pub omitted: Vec<Ident>,
    /// Digest over the references and the pinned dependency receipts.
    pub digest: Digest,
}

impl Manifest {
    /// The references alone, for the attempt record.
    pub fn references(&self) -> Vec<EvidenceRef> {
        self.evidence.iter().map(|r| r.reference.clone()).collect()
    }
}

/// Why a port could not be resolved, or a frozen manifest no longer holds.
#[derive(Debug)]
pub enum EvidenceError {
    /// The narrowing is not a narrowing.
    Narrowing(NarrowingError),
    /// An offer names a port the node does not declare.
    UnknownPort(Ident),
    /// An offer names a port the retry narrowing dropped.
    DroppedPort(Ident),
    /// Two offers name the same port.
    DuplicateOffer(Ident),
    /// A required port has no offer.
    MissingRequired(Ident),
    /// This build cannot fill the port's kind.
    Unsupported {
        /// The port.
        port: Ident,
        /// Its kind.
        kind: PortKind,
    },
    /// Proof evidence comes from pinned receipts, never from an offer.
    ProofsNotOffered(Ident),
    /// No pinned receipt covers the proof port's scope.
    ProofMissing {
        /// The port.
        port: Ident,
        /// The scope it declares.
        scope: Vec<String>,
    },
    /// More than one pinned receipt covers the proof port's scope.
    AmbiguousProof(Ident),
    /// The offered source is not in the port's allowed scope.
    UndeclaredSource {
        /// The port.
        port: Ident,
        /// The source.
        source: String,
    },
    /// The scope allows the source but the run has not registered it.
    UnregisteredSource {
        /// The port.
        port: Ident,
        /// The source.
        source: String,
    },
    /// The registered source cannot feed the port's kind.
    SourceKindMismatch {
        /// The port.
        port: Ident,
        /// The source.
        source: String,
        /// The port kind.
        kind: PortKind,
    },
    /// The offered item is not what the port's kind takes.
    ItemKindMismatch(Ident),
    /// The port expects a type this resolver does not produce for it.
    TypeMismatch {
        /// The port.
        port: Ident,
        /// The declared type.
        expected: Box<TypeRef>,
        /// What the resolver produces.
        produced: &'static str,
    },
    /// A path is not a clean relative path.
    PathNotRelative {
        /// The port.
        port: Ident,
        /// The path.
        path: String,
    },
    /// A path resolves outside its source directory.
    PathEscapes {
        /// The port.
        port: Ident,
        /// The path.
        path: String,
    },
    /// A path is not a regular file.
    NotAFile {
        /// The port.
        port: Ident,
        /// The path.
        path: String,
    },
    /// A path could not be read.
    Io {
        /// The port.
        port: Ident,
        /// The path.
        path: String,
        /// The error.
        error: io::Error,
    },
    /// A file exceeds the per-file ceiling.
    TooLarge {
        /// The port.
        port: Ident,
        /// The path.
        path: String,
        /// Its size.
        size: u64,
        /// Bytes allowed.
        max: u64,
    },
    /// Reading the port would exceed the total ceiling.
    TotalTooLarge {
        /// The port.
        port: Ident,
        /// Bytes allowed across every port.
        max: u64,
    },
    /// The bytes do not have the digest they must have.
    DigestMismatch {
        /// The port.
        port: Ident,
        /// The digest expected.
        expected: Digest,
        /// The digest found.
        actual: Digest,
    },
    /// The candidate behind a frozen code reference changed.
    SubjectChanged {
        /// The port.
        port: Ident,
        /// The subject frozen.
        expected: String,
        /// The subject now.
        actual: String,
    },
    /// A source's attestation is missing or does not match its digest.
    InvalidAttestation {
        /// The source.
        source: String,
        /// Why.
        reason: String,
    },
    /// The source or evidence is past its validity.
    Expired {
        /// The port.
        port: Ident,
        /// When validity ended.
        valid_until: Timestamp,
    },
    /// The candidate could not be captured.
    Capture {
        /// The port.
        port: Ident,
        /// The error.
        error: CaptureError,
    },
    /// A frozen locator is not in the form this build writes.
    BadLocator {
        /// The port.
        port: Ident,
        /// The locator.
        locator: String,
    },
    /// Canonical bytes could not be produced.
    Canonical(CanonicalError),
}

fn kind_name(kind: PortKind) -> &'static str {
    match kind {
        PortKind::Rag => "rag",
        PortKind::Human => "human",
        PortKind::Logs => "logs",
        PortKind::Code => "code",
        PortKind::Proofs => "proofs",
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvidenceError::Narrowing(e) => write!(f, "{e}"),
            EvidenceError::UnknownPort(port) => write!(f, "port {port} is not declared"),
            EvidenceError::DroppedPort(port) => {
                write!(f, "port {port} was dropped by the retry narrowing")
            }
            EvidenceError::DuplicateOffer(port) => write!(f, "port {port} is offered twice"),
            EvidenceError::MissingRequired(port) => {
                write!(f, "required port {port} has no evidence")
            }
            EvidenceError::Unsupported { port, kind } => write!(
                f,
                "port {port} is a {} port; this build cannot resolve {} evidence",
                kind_name(*kind),
                kind_name(*kind)
            ),
            EvidenceError::ProofsNotOffered(port) => write!(
                f,
                "proof port {port} is filled from pinned receipts, not from an offer"
            ),
            EvidenceError::ProofMissing { port, scope } => {
                write!(f, "proof port {port} has no pinned receipt for {scope:?}")
            }
            EvidenceError::AmbiguousProof(port) => {
                write!(f, "proof port {port} matches more than one pinned receipt")
            }
            EvidenceError::UndeclaredSource { port, source } => {
                write!(f, "source {source:?} is outside the scope of port {port}")
            }
            EvidenceError::UnregisteredSource { port, source } => write!(
                f,
                "source {source:?} allowed for port {port} is not registered for this run"
            ),
            EvidenceError::SourceKindMismatch { port, source, kind } => write!(
                f,
                "source {source:?} cannot feed {} port {port}",
                kind_name(*kind)
            ),
            EvidenceError::ItemKindMismatch(port) => {
                write!(
                    f,
                    "the offer for port {port} is not the item its kind takes"
                )
            }
            EvidenceError::TypeMismatch {
                port,
                expected,
                produced,
            } => write!(
                f,
                "port {port} expects {}@{} with digest {}; this build produces {produced}",
                expected.schema_id, expected.version, expected.digest
            ),
            EvidenceError::PathNotRelative { port, path } => {
                write!(f, "port {port}: {path:?} is not a clean relative path")
            }
            EvidenceError::PathEscapes { port, path } => {
                write!(f, "port {port}: {path:?} resolves outside its source")
            }
            EvidenceError::NotAFile { port, path } => {
                write!(f, "port {port}: {path:?} is not a regular file")
            }
            EvidenceError::Io { port, path, error } => {
                write!(f, "port {port}: cannot read {path:?}: {error}")
            }
            EvidenceError::TooLarge {
                port,
                path,
                size,
                max,
            } => write!(
                f,
                "port {port}: {path:?} is {size} bytes, above the {max}-byte ceiling"
            ),
            EvidenceError::TotalTooLarge { port, max } => write!(
                f,
                "port {port}: reading it would exceed the {max}-byte total ceiling"
            ),
            EvidenceError::DigestMismatch {
                port,
                expected,
                actual,
            } => write!(f, "port {port}: expected {expected}, found {actual}"),
            EvidenceError::SubjectChanged {
                port,
                expected,
                actual,
            } => write!(
                f,
                "port {port}: the candidate changed from {expected} to {actual}"
            ),
            EvidenceError::InvalidAttestation { source, reason } => {
                write!(f, "source {source:?}: attestation invalid: {reason}")
            }
            EvidenceError::Expired { port, valid_until } => {
                write!(f, "port {port}: evidence expired at {valid_until}")
            }
            EvidenceError::Capture { port, error } => write!(f, "port {port}: {error}"),
            EvidenceError::BadLocator { port, locator } => {
                write!(
                    f,
                    "port {port}: locator {locator:?} is not readable by this build"
                )
            }
            EvidenceError::Canonical(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EvidenceError {}

/// Producer recorded for evidence the code adapter captured.
pub const CODE_PRODUCER: &str = "checkspan/code-adapter";

/// Resolves and re-verifies evidence for one run against its registered
/// sources.
#[derive(Debug, Clone, Copy)]
pub struct Resolver<'a> {
    sources: &'a Sources,
    limits: &'a Limits,
    now: &'a Timestamp,
}

impl<'a> Resolver<'a> {
    /// A resolver over `sources`, bounded by `limits`, at moment `now`.
    pub fn new(sources: &'a Sources, limits: &'a Limits, now: &'a Timestamp) -> Self {
        Self {
            sources,
            limits,
            now,
        }
    }

    /// Fill every evidence port of `node` (after `narrowing`, if any) from
    /// `offers` and the pinned dependency snapshot, or report every reason
    /// the node cannot be dispatched.
    pub fn resolve(
        &self,
        node: &NodeSpec,
        narrowing: Option<&Narrowing>,
        dependencies: &DependencySnapshot,
        offers: &[Offer],
    ) -> Result<Manifest, Vec<EvidenceError>> {
        let ports = match narrowing {
            Some(narrowing) => {
                check_narrowing(node, narrowing).map_err(|e| vec![EvidenceError::Narrowing(e)])?
            }
            None => node.evidence_ports.clone(),
        };
        let ports: BTreeMap<&Ident, &PortSpec> = ports.iter().map(|p| (&p.name, p)).collect();
        let mut errors = Vec::new();

        let mut offered: BTreeMap<&Ident, &Offer> = BTreeMap::new();
        for offer in offers {
            if !ports.contains_key(&offer.port) {
                if node.evidence_ports.iter().any(|p| p.name == offer.port) {
                    errors.push(EvidenceError::DroppedPort(offer.port.clone()));
                } else {
                    errors.push(EvidenceError::UnknownPort(offer.port.clone()));
                }
                continue;
            }
            if offered.insert(&offer.port, offer).is_some() {
                errors.push(EvidenceError::DuplicateOffer(offer.port.clone()));
            }
        }

        let mut evidence = Vec::new();
        let mut omitted = Vec::new();
        let mut remaining = self.limits.max_total_bytes;
        for (name, port) in &ports {
            let offer = offered.get(name).copied();
            let outcome = match port.kind {
                PortKind::Proofs => match offer {
                    Some(_) => Err(EvidenceError::ProofsNotOffered((*name).clone())),
                    None => self.resolve_proof(port, dependencies).map(Some),
                },
                PortKind::Rag | PortKind::Human => match (offer, port.required) {
                    (None, false) => Ok(None),
                    _ => Err(EvidenceError::Unsupported {
                        port: (*name).clone(),
                        kind: port.kind,
                    }),
                },
                PortKind::Code | PortKind::Logs => match (offer, port.required) {
                    (None, true) => Err(EvidenceError::MissingRequired((*name).clone())),
                    (None, false) => Ok(None),
                    (Some(offer), _) => self.resolve_offer(port, offer, &mut remaining).map(Some),
                },
            };
            match outcome {
                Ok(Some(resolved)) => evidence.push(resolved),
                Ok(None) => omitted.push((*name).clone()),
                Err(e) => errors.push(e),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let references: Vec<EvidenceRef> = evidence.iter().map(|r| r.reference.clone()).collect();
        let digest = input_manifest_digest(&references, &dependencies.receipt_refs())
            .map_err(|e| vec![EvidenceError::Canonical(e)])?;
        Ok(Manifest {
            evidence,
            omitted,
            digest,
        })
    }

    /// Check that a frozen manifest still holds: every reference names a
    /// declared port, its source is still registered and in scope, the
    /// candidate or bytes behind it are unchanged, nothing is past its
    /// validity, and every proof is still pinned. Empty means it holds.
    pub fn verify(
        &self,
        node: &NodeSpec,
        manifest: &[EvidenceRef],
        dependencies: &DependencySnapshot,
    ) -> Vec<EvidenceError> {
        let mut errors = Vec::new();
        for reference in manifest {
            let port = &reference.port_name;
            let Some(spec) = node.evidence_ports.iter().find(|p| p.name == *port) else {
                errors.push(EvidenceError::UnknownPort(port.clone()));
                continue;
            };
            if let Some(valid_until) = &reference.valid_until
                && valid_until.is_before(self.now)
            {
                errors.push(EvidenceError::Expired {
                    port: port.clone(),
                    valid_until: valid_until.clone(),
                });
            }
            let outcome = match spec.kind {
                PortKind::Code => self.verify_code(spec, reference),
                PortKind::Logs => self.verify_log(spec, reference),
                PortKind::Proofs => self.verify_proof(spec, reference, dependencies),
                PortKind::Rag | PortKind::Human => Err(EvidenceError::Unsupported {
                    port: port.clone(),
                    kind: spec.kind,
                }),
            };
            if let Err(e) = outcome {
                errors.push(e);
            }
        }
        errors
    }

    fn resolve_proof(
        &self,
        port: &PortSpec,
        dependencies: &DependencySnapshot,
    ) -> Result<Resolved, EvidenceError> {
        let mut matches = dependencies.resolved.iter().filter(|r| {
            port.allowed_source_scope
                .iter()
                .any(|scope| scope_names_node(scope, r.dependency.node.node_id.as_str()))
        });
        let Some(resolved) = matches.next() else {
            return Err(EvidenceError::ProofMissing {
                port: port.name.clone(),
                scope: port.allowed_source_scope.clone(),
            });
        };
        if matches.next().is_some() {
            return Err(EvidenceError::AmbiguousProof(port.name.clone()));
        }
        let producer = match resolved.provenance {
            DependencyProvenance::SameRun => format!("run:{}", resolved.receipt.run_id),
            DependencyProvenance::Imported => format!("import:{}", resolved.receipt.run_id),
        };
        Ok(Resolved {
            reference: EvidenceRef {
                port_name: port.name.clone(),
                locator: format!("receipt:{}", resolved.receipt.id),
                subject: format!("node:{}", resolved.receipt.node),
                version: resolved.receipt.id.to_string(),
                content_digest: resolved.result_digest.clone(),
                provenance: Provenance {
                    producer,
                    attestation_ref: Some(format!("receipt:{}", resolved.receipt.id)),
                },
                policy_ref: port.handling_policy_ref.clone(),
                valid_until: None,
            },
            bytes: None,
        })
    }

    fn source_for(&self, port: &PortSpec, name: &str) -> Result<&'a Source, EvidenceError> {
        if !port.allowed_source_scope.iter().any(|s| s == name) {
            return Err(EvidenceError::UndeclaredSource {
                port: port.name.clone(),
                source: name.to_owned(),
            });
        }
        self.sources
            .get(name)
            .ok_or_else(|| EvidenceError::UnregisteredSource {
                port: port.name.clone(),
                source: name.to_owned(),
            })
    }

    fn resolve_offer(
        &self,
        port: &PortSpec,
        offer: &Offer,
        remaining: &mut u64,
    ) -> Result<Resolved, EvidenceError> {
        let source = self.source_for(port, &offer.source)?;
        let mismatch = || EvidenceError::SourceKindMismatch {
            port: port.name.clone(),
            source: offer.source.clone(),
            kind: port.kind,
        };
        match (port.kind, source, &offer.item) {
            (PortKind::Code, Source::Repository { root }, Item::Candidate(selection)) => {
                self.resolve_code(port, root, selection, remaining)
            }
            (PortKind::Code, Source::Repository { .. }, Item::File { .. }) => {
                Err(EvidenceError::ItemKindMismatch(port.name.clone()))
            }
            (
                PortKind::Logs,
                Source::Directory {
                    root,
                    producer,
                    attestation,
                    valid_until,
                },
                Item::File {
                    path,
                    subject,
                    version,
                    expected_digest,
                },
            ) => {
                check_attestation(&port.name, &offer.source, root, attestation.as_ref())?;
                if let Some(until) = valid_until
                    && until.is_before(self.now)
                {
                    return Err(EvidenceError::Expired {
                        port: port.name.clone(),
                        valid_until: until.clone(),
                    });
                }
                let bytes = read_contained(
                    &port.name,
                    root,
                    path,
                    self.limits.max_file_bytes,
                    remaining,
                    self.limits.max_total_bytes,
                )?;
                let digest = artifact_digest(&bytes);
                if let Some(expected) = expected_digest
                    && *expected != digest
                {
                    return Err(EvidenceError::DigestMismatch {
                        port: port.name.clone(),
                        expected: expected.clone(),
                        actual: digest,
                    });
                }
                Ok(Resolved {
                    reference: EvidenceRef {
                        port_name: port.name.clone(),
                        locator: format!("file:{}/{path}", offer.source),
                        subject: subject.clone(),
                        version: version.clone(),
                        content_digest: digest,
                        provenance: Provenance {
                            producer: producer.clone(),
                            attestation_ref: attestation
                                .as_ref()
                                .map(|a| format!("file:{}/{}", offer.source, a.reference)),
                        },
                        policy_ref: port.handling_policy_ref.clone(),
                        valid_until: valid_until.clone(),
                    },
                    bytes: Some(bytes),
                })
            }
            (PortKind::Logs, Source::Directory { .. }, Item::Candidate(_)) => {
                Err(EvidenceError::ItemKindMismatch(port.name.clone()))
            }
            _ => Err(mismatch()),
        }
    }

    fn resolve_code(
        &self,
        port: &PortSpec,
        root: &Path,
        selection: &Selection,
        remaining: &mut u64,
    ) -> Result<Resolved, EvidenceError> {
        check_code_type(port)?;
        let captured =
            code::capture(root, selection, &self.limits.capture, self.now).map_err(|error| {
                EvidenceError::Capture {
                    port: port.name.clone(),
                    error,
                }
            })?;
        let value = serde_json::to_value(&captured.result)
            .map_err(|e| EvidenceError::Canonical(CanonicalError::Serialize(e.to_string())))?;
        let bytes = jcs_bytes(&value).map_err(EvidenceError::Canonical)?;
        if bytes.len() as u64 > *remaining {
            return Err(EvidenceError::TotalTooLarge {
                port: port.name.clone(),
                max: self.limits.max_total_bytes,
            });
        }
        *remaining -= bytes.len() as u64;
        let candidate = &captured.result.candidate;
        Ok(Resolved {
            reference: EvidenceRef {
                port_name: port.name.clone(),
                locator: code_locator(
                    &captured.root,
                    &captured.result.base_commit,
                    candidate.kind,
                    &candidate.commit,
                ),
                subject: format!("patch_subject:{}", captured.subject_digest),
                version: candidate.commit.to_string(),
                content_digest: artifact_digest(&bytes),
                provenance: Provenance {
                    producer: CODE_PRODUCER.to_owned(),
                    attestation_ref: None,
                },
                policy_ref: port.handling_policy_ref.clone(),
                valid_until: None,
            },
            bytes: Some(bytes),
        })
    }

    fn verify_code(&self, port: &PortSpec, reference: &EvidenceRef) -> Result<(), EvidenceError> {
        check_code_type(port)?;
        let (root, selection) = parse_code_locator(&port.name, &reference.locator)?;
        let registered = port.allowed_source_scope.iter().any(|name| {
            matches!(
                self.sources.get(name),
                Some(Source::Repository { root: r }) if same_directory(r, &root)
            )
        });
        if !registered {
            return Err(EvidenceError::UnregisteredSource {
                port: port.name.clone(),
                source: root.display().to_string(),
            });
        }
        let captured =
            code::capture(&root, &selection, &self.limits.capture, self.now).map_err(|error| {
                EvidenceError::Capture {
                    port: port.name.clone(),
                    error,
                }
            })?;
        let actual = format!("patch_subject:{}", captured.subject_digest);
        if actual != reference.subject {
            return Err(EvidenceError::SubjectChanged {
                port: port.name.clone(),
                expected: reference.subject.clone(),
                actual,
            });
        }
        Ok(())
    }

    fn verify_log(&self, port: &PortSpec, reference: &EvidenceRef) -> Result<(), EvidenceError> {
        let bad = || EvidenceError::BadLocator {
            port: port.name.clone(),
            locator: reference.locator.clone(),
        };
        let rest = reference.locator.strip_prefix("file:").ok_or_else(bad)?;
        let (name, path) = rest.split_once('/').ok_or_else(bad)?;
        let source = self.source_for(port, name)?;
        let Source::Directory {
            root,
            attestation,
            valid_until,
            ..
        } = source
        else {
            return Err(EvidenceError::SourceKindMismatch {
                port: port.name.clone(),
                source: name.to_owned(),
                kind: port.kind,
            });
        };
        check_attestation(&port.name, name, root, attestation.as_ref())?;
        if let Some(until) = valid_until
            && until.is_before(self.now)
        {
            return Err(EvidenceError::Expired {
                port: port.name.clone(),
                valid_until: until.clone(),
            });
        }
        let mut remaining = self.limits.max_total_bytes;
        let bytes = read_contained(
            &port.name,
            root,
            path,
            self.limits.max_file_bytes,
            &mut remaining,
            self.limits.max_total_bytes,
        )?;
        let actual = artifact_digest(&bytes);
        if actual != reference.content_digest {
            return Err(EvidenceError::DigestMismatch {
                port: port.name.clone(),
                expected: reference.content_digest.clone(),
                actual,
            });
        }
        Ok(())
    }

    fn verify_proof(
        &self,
        port: &PortSpec,
        reference: &EvidenceRef,
        dependencies: &DependencySnapshot,
    ) -> Result<(), EvidenceError> {
        let Some(id) = reference.locator.strip_prefix("receipt:") else {
            return Err(EvidenceError::BadLocator {
                port: port.name.clone(),
                locator: reference.locator.clone(),
            });
        };
        let Some(resolved) = dependencies
            .resolved
            .iter()
            .find(|r| r.receipt.id.as_str() == id)
        else {
            return Err(EvidenceError::ProofMissing {
                port: port.name.clone(),
                scope: port.allowed_source_scope.clone(),
            });
        };
        if resolved.result_digest != reference.content_digest {
            return Err(EvidenceError::DigestMismatch {
                port: port.name.clone(),
                expected: reference.content_digest.clone(),
                actual: resolved.result_digest.clone(),
            });
        }
        Ok(())
    }
}

/// True when a proof scope entry `node:<id>` names `node_id`.
fn scope_names_node(scope: &str, node_id: &str) -> bool {
    scope.strip_prefix("node:") == Some(node_id)
}

/// A `code` port must expect exactly the patch result record this build
/// produces, down to the bundled schema's digest.
fn check_code_type(port: &PortSpec) -> Result<(), EvidenceError> {
    let schema = registry::bundled(PatchResult::SCHEMA_ID).expect("patch result schema is bundled");
    let expected = &port.expected_type;
    let matches = expected.schema_id.as_str() == PatchResult::KIND.as_str()
        && expected.version.get() == PatchResult::VERSION.0
        && expected.digest == artifact_digest(schema.as_bytes());
    if matches {
        Ok(())
    } else {
        Err(EvidenceError::TypeMismatch {
            port: port.name.clone(),
            expected: Box::new(expected.clone()),
            produced: "patch_result@1 with the bundled schema digest",
        })
    }
}

fn code_locator(root: &Path, base: &CommitId, kind: CandidateKind, commit: &CommitId) -> String {
    format!(
        "git:{}#base={base}&candidate={}:{commit}",
        root.display(),
        kind.as_str()
    )
}

fn parse_code_locator(port: &Ident, locator: &str) -> Result<(PathBuf, Selection), EvidenceError> {
    let bad = || EvidenceError::BadLocator {
        port: port.clone(),
        locator: locator.to_owned(),
    };
    let rest = locator.strip_prefix("git:").ok_or_else(bad)?;
    let (root, query) = rest.rsplit_once('#').ok_or_else(bad)?;
    let (base, candidate) = query.split_once('&').ok_or_else(bad)?;
    let base = base.strip_prefix("base=").ok_or_else(bad)?;
    let candidate = candidate.strip_prefix("candidate=").ok_or_else(bad)?;
    let (kind, commit) = candidate.split_once(':').ok_or_else(bad)?;
    let commit = CommitId::new(commit).map_err(|_| bad())?;
    CommitId::new(base).map_err(|_| bad())?;
    let selector = match kind {
        "commit" => Selector::Commit(commit.to_string()),
        "working_tree" => Selector::WorkingTree,
        _ => return Err(bad()),
    };
    Ok((
        PathBuf::from(root),
        Selection {
            candidate: selector,
            base: Some(base.to_owned()),
        },
    ))
}

fn same_directory(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The source's attestation document must exist inside the source and have
/// exactly the declared digest.
fn check_attestation(
    port: &Ident,
    source: &str,
    root: &Path,
    attestation: Option<&Attestation>,
) -> Result<(), EvidenceError> {
    let Some(attestation) = attestation else {
        return Ok(());
    };
    let invalid = |reason: String| EvidenceError::InvalidAttestation {
        source: source.to_owned(),
        reason,
    };
    let mut remaining = u64::MAX;
    let bytes = read_contained(
        port,
        root,
        &attestation.reference,
        u64::MAX,
        &mut remaining,
        u64::MAX,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let actual = artifact_digest(&bytes);
    if actual != attestation.digest {
        return Err(invalid(format!(
            "{} has digest {actual}, not {}",
            attestation.reference, attestation.digest
        )));
    }
    Ok(())
}

/// Read a regular file that `path` names inside `root` after every link is
/// resolved, within the per-file and remaining-total ceilings.
fn read_contained(
    port: &Ident,
    root: &Path,
    path: &str,
    max_file: u64,
    remaining: &mut u64,
    max_total: u64,
) -> Result<Vec<u8>, EvidenceError> {
    if !is_clean_relative_path(path) {
        return Err(EvidenceError::PathNotRelative {
            port: port.clone(),
            path: path.to_owned(),
        });
    }
    let io = |error| EvidenceError::Io {
        port: port.clone(),
        path: path.to_owned(),
        error,
    };
    let canonical_root = fs::canonicalize(root).map_err(io)?;
    let full = fs::canonicalize(root.join(path)).map_err(io)?;
    if !full.starts_with(&canonical_root) {
        return Err(EvidenceError::PathEscapes {
            port: port.clone(),
            path: path.to_owned(),
        });
    }
    let metadata = fs::metadata(&full).map_err(io)?;
    if !metadata.is_file() {
        return Err(EvidenceError::NotAFile {
            port: port.clone(),
            path: path.to_owned(),
        });
    }
    if metadata.len() > max_file {
        return Err(EvidenceError::TooLarge {
            port: port.clone(),
            path: path.to_owned(),
            size: metadata.len(),
            max: max_file,
        });
    }
    if metadata.len() > *remaining {
        return Err(EvidenceError::TotalTooLarge {
            port: port.clone(),
            max: max_total,
        });
    }
    let bytes = fs::read(&full).map_err(io)?;
    let size = bytes.len() as u64;
    if size > max_file || size > *remaining {
        return Err(EvidenceError::TooLarge {
            port: port.clone(),
            path: path.to_owned(),
            size,
            max: max_file.min(*remaining),
        });
    }
    *remaining -= size;
    Ok(bytes)
}
