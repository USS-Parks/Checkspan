//! The exact local candidate a software check runs against.
//!
//! A `patch_result` names a repository by its root commits, a base commit,
//! and a candidate: either a commit, or a working tree that sits on a commit
//! and carries uncommitted changes. Every changed path carries the digest of
//! its exact content. The record establishes that a typed candidate exists
//! against the pinned base; it says nothing about whether the candidate is
//! any good.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ids::{Digest, InvalidValue, Timestamp};
use super::record::{Record, RecordKind, SchemaVersion};

/// Largest integer an envelope digest can carry exactly.
const MAX_EXACT_INTEGER: u64 = (1 << 53) - 1;

/// A Git object name: 40 (SHA-1) or 64 (SHA-256 repository) lowercase hex
/// digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct CommitId(String);

impl CommitId {
    /// Build a commit id from its hex text.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidValue> {
        let value = value.into();
        let well_formed = (value.len() == 40 || value.len() == 64)
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if well_formed {
            Ok(Self(value))
        } else {
            Err(InvalidValue {
                kind: "commit id",
                reason: format!("{value:?} is not 40 or 64 lowercase hex digits"),
            })
        }
    }

    /// The hex text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CommitId {
    type Error = InvalidValue;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The repository a candidate belongs to, named by its root commits so that
/// every clone agrees on it without a path or a remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    /// Every parentless commit reachable from the candidate, sorted.
    pub root_commits: Vec<CommitId>,
}

/// What kind of thing the candidate is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    /// Exactly the named commit.
    Commit,
    /// The working tree on top of the named commit, with uncommitted changes.
    WorkingTree,
}

impl CandidateKind {
    /// The wire text.
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateKind::Commit => "commit",
            CandidateKind::WorkingTree => "working_tree",
        }
    }
}

/// The candidate: a commit, or a working tree resting on one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    /// Commit or working tree.
    pub kind: CandidateKind,
    /// The candidate commit, or the commit the working tree rests on.
    pub commit: CommitId,
    /// Digest of the textual patch from the base to the candidate, produced
    /// under pinned diff options. Untracked files are not part of a Git
    /// patch; they are identified through `changes` instead.
    pub patch_digest: Digest,
}

/// How a path differs from the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Tracked in the candidate, absent from the base.
    Added,
    /// Present in both with different content or type.
    Modified,
    /// Present in the base, absent from the candidate.
    Deleted,
    /// Present in the working tree but not tracked by Git.
    Untracked,
}

impl ChangeKind {
    /// The wire text.
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Modified => "modified",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Untracked => "untracked",
        }
    }
}

/// One changed path and the exact content it has in the candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// Path relative to the repository root, `/`-separated, as Git reports it.
    pub path: String,
    /// How the path differs from the base.
    pub change: ChangeKind,
    /// Digest of the candidate content; absent exactly for deletions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<Digest>,
    /// Size in bytes of the candidate content; absent exactly for deletions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// The tool that read the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    /// Always `git`.
    pub name: String,
    /// The version the tool reported.
    pub version: String,
}

/// The exact local candidate a software check runs against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchResult {
    /// Always `patch_result`.
    pub record: RecordKind,
    /// Always `1` for this build.
    pub schema_version: SchemaVersion,
    /// The repository.
    pub repository: Repository,
    /// The commit the candidate is compared with.
    pub base_commit: CommitId,
    /// The candidate.
    pub candidate: Candidate,
    /// Every path that differs between the base and the candidate, sorted by
    /// path, with the candidate content identified by digest.
    pub changes: Vec<Change>,
    /// When the candidate was read.
    pub captured_at: Timestamp,
    /// What read it.
    pub tool: Tool,
}

impl Record for PatchResult {
    const KIND: RecordKind = RecordKind::PatchResult;
    const VERSION: SchemaVersion = SchemaVersion(1);
    const SCHEMA_ID: &'static str = "https://checkspan.invalid/schemas/v1/patch-result.schema.json";
}

/// The identity-bearing part of a patch result: everything except when it
/// was captured and by which tool version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatchSubject<'a> {
    /// The repository.
    pub repository: &'a Repository,
    /// The base commit.
    pub base_commit: &'a CommitId,
    /// The candidate.
    pub candidate: &'a Candidate,
    /// The changed paths and their content digests.
    pub changes: &'a [Change],
}

/// Why a patch result is not internally consistent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchError {
    /// The repository lists no root commit.
    NoRootCommits,
    /// Root commits are not strictly ascending.
    UnsortedRootCommits,
    /// A path is not a clean relative `/`-separated path.
    PathNotRelative(String),
    /// Paths are not strictly ascending, so one repeats or the order is
    /// not canonical.
    UnsortedPaths(String),
    /// A deletion carries content.
    DeletedWithContent(String),
    /// A present path lacks its content digest or size.
    MissingContent(String),
    /// A size exceeds what an envelope digest can carry exactly.
    SizeNotExact(String),
    /// The tool is not `git`.
    UnknownTool(String),
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatchError::NoRootCommits => f.write_str("repository lists no root commit"),
            PatchError::UnsortedRootCommits => {
                f.write_str("root commits are not sorted and unique")
            }
            PatchError::PathNotRelative(path) => {
                write!(f, "path {path:?} is not a clean relative path")
            }
            PatchError::UnsortedPaths(path) => {
                write!(f, "path {path:?} is out of order or repeated")
            }
            PatchError::DeletedWithContent(path) => {
                write!(f, "deleted path {path:?} carries content")
            }
            PatchError::MissingContent(path) => {
                write!(f, "path {path:?} lacks its content digest or size")
            }
            PatchError::SizeNotExact(path) => {
                write!(f, "size of {path:?} exceeds 2^53 - 1")
            }
            PatchError::UnknownTool(name) => write!(f, "tool {name:?} is not git"),
        }
    }
}

impl std::error::Error for PatchError {}

/// True for a `/`-separated path with no empty, `.`, or `..` segment, no
/// leading `/`, and no backslash.
pub fn is_clean_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

impl PatchResult {
    /// Every internal inconsistency, in document order.
    pub fn check_bindings(&self) -> Vec<PatchError> {
        let mut errors = Vec::new();
        if self.repository.root_commits.is_empty() {
            errors.push(PatchError::NoRootCommits);
        }
        if !self
            .repository
            .root_commits
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        {
            errors.push(PatchError::UnsortedRootCommits);
        }
        let mut previous: Option<&str> = None;
        for change in &self.changes {
            if !is_clean_relative_path(&change.path) {
                errors.push(PatchError::PathNotRelative(change.path.clone()));
            }
            if previous.is_some_and(|p| p >= change.path.as_str()) {
                errors.push(PatchError::UnsortedPaths(change.path.clone()));
            }
            previous = Some(&change.path);
            let has_content = change.content_digest.is_some() || change.size.is_some();
            let complete = change.content_digest.is_some() && change.size.is_some();
            match change.change {
                ChangeKind::Deleted if has_content => {
                    errors.push(PatchError::DeletedWithContent(change.path.clone()));
                }
                ChangeKind::Deleted => {}
                _ if !complete => errors.push(PatchError::MissingContent(change.path.clone())),
                _ => {}
            }
            if change.size.is_some_and(|size| size > MAX_EXACT_INTEGER) {
                errors.push(PatchError::SizeNotExact(change.path.clone()));
            }
        }
        if self.tool.name != "git" {
            errors.push(PatchError::UnknownTool(self.tool.name.clone()));
        }
        errors
    }

    /// The identity-bearing part of the record.
    pub fn subject(&self) -> PatchSubject<'_> {
        PatchSubject {
            repository: &self.repository,
            base_commit: &self.base_commit,
            candidate: &self.candidate,
            changes: &self.changes,
        }
    }
}
