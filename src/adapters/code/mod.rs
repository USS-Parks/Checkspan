//! Capture of exact local patch subjects from a Git repository.
//!
//! Everything here reads. The adapter drives the installed `git` executable
//! with explicit argument vectors, never a shell, asks it to take no optional
//! locks, and never stages, stashes, commits, or checks out. A working tree
//! with uncommitted changes is captured as a `working_tree` candidate: the
//! commit it rests on plus the exact content of every changed path. It is
//! never presented as that commit, and Checkspan never creates a commit to
//! give it one.
//!
//! The subject digest ([`crate::digests::patch_subject_digest`]) is the
//! binding identity of a candidate. The patch digest identifies the textual
//! patch Git produces under pinned diff options; repository attributes such
//! as diff drivers still apply to it, and the Git version is recorded beside
//! it.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::contracts::{
    Candidate, CandidateKind, Change, ChangeKind, CommitId, Digest, PatchError, PatchResult,
    RecordKind, Repository, SchemaVersion, Timestamp, Tool,
};
use crate::digests::{CanonicalError, artifact_digest, patch_subject_digest};

/// Ceilings on what one capture reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum changed paths, untracked files included.
    pub max_paths: usize,
    /// Maximum bytes of candidate content read across all changed paths.
    pub max_total_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_paths: 4096,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Which candidate to capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// The working tree as it is on disk. A clean tree is captured as its
    /// `HEAD` commit; a dirty one as a `working_tree` candidate.
    WorkingTree,
    /// A commit named by any revision expression Git accepts.
    Commit(String),
}

/// What to capture and what to compare it with. Without a base, a working
/// tree is compared with `HEAD` and a commit with its first parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// The candidate.
    pub candidate: Selector,
    /// The base revision, if not the default.
    pub base: Option<String>,
}

/// A captured candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    /// The repository's top-level directory.
    pub root: PathBuf,
    /// The record.
    pub result: PatchResult,
    /// The binding identity of the candidate.
    pub subject_digest: Digest,
}

/// Why a candidate could not be captured.
#[derive(Debug)]
pub enum CaptureError {
    /// `git` could not be started.
    GitUnavailable(io::Error),
    /// The path is not inside a Git repository.
    NotARepository(PathBuf),
    /// A Git command failed.
    Git {
        /// The command, without the pinned global options.
        command: String,
        /// Its exit status, when it exited.
        status: Option<i32>,
        /// Its standard error, bounded.
        stderr: String,
    },
    /// The revision does not name a commit.
    UnknownRevision(String),
    /// The candidate commit has no parent, so a base must be named.
    NoParent(CommitId),
    /// A path is in an unmerged state.
    Unmerged(String),
    /// Git produced output the adapter cannot read.
    Malformed(&'static str),
    /// More changed paths than the limit allows.
    TooManyPaths {
        /// Paths found.
        count: usize,
        /// Paths allowed.
        max: usize,
    },
    /// Reading this path would exceed the content limit.
    TooLarge {
        /// The path.
        path: String,
        /// Its size.
        size: u64,
        /// Bytes allowed across all paths.
        max: u64,
    },
    /// A path cannot be captured.
    Unsupported {
        /// The path.
        path: String,
        /// Why.
        reason: &'static str,
    },
    /// A working-tree path could not be read.
    Io {
        /// The path.
        path: String,
        /// The error.
        error: io::Error,
    },
    /// The subject digest could not be computed.
    Canonical(CanonicalError),
    /// The assembled record failed its own checks.
    Contract(Vec<PatchError>),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::GitUnavailable(e) => write!(f, "cannot start git: {e}"),
            CaptureError::NotARepository(path) => {
                write!(f, "{} is not inside a git repository", path.display())
            }
            CaptureError::Git {
                command,
                status,
                stderr,
            } => match status {
                Some(code) => write!(f, "{command} exited with {code}: {}", stderr.trim()),
                None => write!(f, "{command} was terminated: {}", stderr.trim()),
            },
            CaptureError::UnknownRevision(rev) => write!(f, "{rev:?} does not name a commit"),
            CaptureError::NoParent(commit) => {
                write!(f, "commit {commit} has no parent; name a base explicitly")
            }
            CaptureError::Unmerged(path) => write!(f, "path {path:?} is unmerged"),
            CaptureError::Malformed(what) => write!(f, "unreadable git output: {what}"),
            CaptureError::TooManyPaths { count, max } => {
                write!(f, "{count} changed paths exceed the limit of {max}")
            }
            CaptureError::TooLarge { path, size, max } => write!(
                f,
                "reading {path:?} ({size} bytes) would exceed the content limit of {max} bytes"
            ),
            CaptureError::Unsupported { path, reason } => {
                write!(f, "path {path:?} is {reason}")
            }
            CaptureError::Io { path, error } => write!(f, "cannot read {path:?}: {error}"),
            CaptureError::Canonical(e) => write!(f, "{e}"),
            CaptureError::Contract(errors) => {
                write!(f, "captured record is inconsistent:")?;
                for e in errors {
                    write!(f, " {e};")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<CanonicalError> for CaptureError {
    fn from(e: CanonicalError) -> Self {
        CaptureError::Canonical(e)
    }
}

/// Configuration pinned on every Git invocation so that user settings cannot
/// change what is captured.
const PINNED_CONFIG: &[&str] = &[
    "core.quotepath=false",
    "diff.algorithm=myers",
    "diff.noprefix=false",
    "diff.mnemonicPrefix=false",
    "diff.suppressBlankEmpty=false",
    "diff.renames=false",
];

/// Longest standard-error excerpt kept in an error.
const MAX_STDERR: usize = 2048;

/// Read the candidate named by `selection` from the repository containing
/// `path`, without writing anything to it.
pub fn capture(
    path: &Path,
    selection: &Selection,
    limits: &Limits,
    now: &Timestamp,
) -> Result<Captured, CaptureError> {
    let git = Git::open(path)?;
    let version = git.version()?;

    let (kind, commit) = match &selection.candidate {
        Selector::WorkingTree => {
            let head = git.resolve("HEAD")?;
            let kind = if git.is_clean()? {
                CandidateKind::Commit
            } else {
                CandidateKind::WorkingTree
            };
            (kind, head)
        }
        Selector::Commit(rev) => (CandidateKind::Commit, git.resolve(rev)?),
    };
    let base = match (&selection.base, &selection.candidate) {
        (Some(rev), _) => git.resolve(rev)?,
        (None, Selector::WorkingTree) => commit.clone(),
        (None, Selector::Commit(_)) => git.first_parent(&commit)?,
    };
    let compare_with = match kind {
        CandidateKind::Commit => Some(&commit),
        CandidateKind::WorkingTree => None,
    };

    let mut paths = git.name_status(&base, compare_with)?;
    if kind == CandidateKind::WorkingTree {
        for path in git.untracked()? {
            paths
                .entry(path)
                .and_modify(|change| *change = ChangeKind::Modified)
                .or_insert(ChangeKind::Untracked);
        }
    }
    if paths.len() > limits.max_paths {
        return Err(CaptureError::TooManyPaths {
            count: paths.len(),
            max: limits.max_paths,
        });
    }

    let mut remaining = limits.max_total_bytes;
    let mut changes = Vec::with_capacity(paths.len());
    for (path, change) in paths {
        let (content_digest, size) = if change == ChangeKind::Deleted {
            (None, None)
        } else {
            let bytes = match kind {
                CandidateKind::Commit => git.blob(&commit, &path, remaining, limits)?,
                CandidateKind::WorkingTree => {
                    read_working_file(&git.root, &path, remaining, limits)?
                }
            };
            let size = bytes.len() as u64;
            remaining -= size;
            (Some(artifact_digest(&bytes)), Some(size))
        };
        changes.push(Change {
            path,
            change,
            content_digest,
            size,
        });
    }

    let patch_digest = artifact_digest(&git.patch(&base, compare_with)?);
    let result = PatchResult {
        record: RecordKind::PatchResult,
        schema_version: SchemaVersion(1),
        repository: Repository {
            root_commits: git.root_commits(&commit)?,
        },
        base_commit: base,
        candidate: Candidate {
            kind,
            commit,
            patch_digest,
        },
        changes,
        captured_at: now.clone(),
        tool: Tool {
            name: "git".to_owned(),
            version,
        },
    };
    let errors = result.check_bindings();
    if !errors.is_empty() {
        return Err(CaptureError::Contract(errors));
    }
    let subject_digest = patch_subject_digest(&result)?;
    Ok(Captured {
        root: git.root,
        result,
        subject_digest,
    })
}

fn read_working_file(
    root: &Path,
    path: &str,
    remaining: u64,
    limits: &Limits,
) -> Result<Vec<u8>, CaptureError> {
    let full = root.join(path);
    let io = |error| CaptureError::Io {
        path: path.to_owned(),
        error,
    };
    let metadata = fs::metadata(&full).map_err(io)?;
    if metadata.is_dir() {
        return Err(CaptureError::Unsupported {
            path: path.to_owned(),
            reason: "a directory; submodules are not captured",
        });
    }
    let too_large = |size| CaptureError::TooLarge {
        path: path.to_owned(),
        size,
        max: limits.max_total_bytes,
    };
    if metadata.len() > remaining {
        return Err(too_large(metadata.len()));
    }
    let bytes = fs::read(&full).map_err(io)?;
    if bytes.len() as u64 > remaining {
        return Err(too_large(bytes.len() as u64));
    }
    Ok(bytes)
}

/// A repository opened for reading.
struct Git {
    root: PathBuf,
}

impl Git {
    fn open(path: &Path) -> Result<Self, CaptureError> {
        match run(path, &["rev-parse", "--show-toplevel"]) {
            Ok(bytes) => Ok(Self {
                root: PathBuf::from(text(&bytes, "top-level path")?.trim_end()),
            }),
            Err(CaptureError::Git { stderr, .. }) if stderr.contains("not a git repository") => {
                Err(CaptureError::NotARepository(path.to_path_buf()))
            }
            Err(e) => Err(e),
        }
    }

    fn run(&self, args: &[&str]) -> Result<Vec<u8>, CaptureError> {
        run(&self.root, args)
    }

    fn version(&self) -> Result<String, CaptureError> {
        let out = self.run(&["--version"])?;
        let line = text(&out, "version")?;
        Ok(line
            .trim()
            .strip_prefix("git version ")
            .unwrap_or(line.trim())
            .to_owned())
    }

    fn resolve(&self, rev: &str) -> Result<CommitId, CaptureError> {
        let spec = format!("{rev}^{{commit}}");
        let out = self
            .run(&["rev-parse", "--verify", "--end-of-options", &spec])
            .map_err(|e| match e {
                CaptureError::Git { .. } => CaptureError::UnknownRevision(rev.to_owned()),
                other => other,
            })?;
        CommitId::new(text(&out, "commit id")?.trim())
            .map_err(|_| CaptureError::Malformed("rev-parse did not print a commit id"))
    }

    fn first_parent(&self, commit: &CommitId) -> Result<CommitId, CaptureError> {
        let spec = format!("{commit}^");
        let out = self
            .run(&["rev-parse", "--verify", "--end-of-options", &spec])
            .map_err(|e| match e {
                CaptureError::Git { .. } => CaptureError::NoParent(commit.clone()),
                other => other,
            })?;
        CommitId::new(text(&out, "commit id")?.trim())
            .map_err(|_| CaptureError::Malformed("rev-parse did not print a commit id"))
    }

    fn is_clean(&self) -> Result<bool, CaptureError> {
        let out = self.run(&["status", "--porcelain=v2", "-z", "--untracked-files=all"])?;
        Ok(out.is_empty())
    }

    /// Changed paths between `base` and `candidate`, or between `base` and
    /// the working tree when no candidate commit is given.
    fn name_status(
        &self,
        base: &CommitId,
        candidate: Option<&CommitId>,
    ) -> Result<BTreeMap<String, ChangeKind>, CaptureError> {
        let mut args = vec!["diff", "--name-status", "--no-renames", "-z", base.as_str()];
        if let Some(candidate) = candidate {
            args.push(candidate.as_str());
        }
        args.push("--");
        let out = self.run(&args)?;
        let tokens: Vec<&[u8]> = out.split(|b| *b == 0).filter(|t| !t.is_empty()).collect();
        if !tokens.len().is_multiple_of(2) {
            return Err(CaptureError::Malformed(
                "name-status output is not in pairs",
            ));
        }
        let mut paths = BTreeMap::new();
        for pair in tokens.chunks(2) {
            let path = path_text(pair[1])?;
            let change = match pair[0] {
                b"A" => ChangeKind::Added,
                b"M" | b"T" => ChangeKind::Modified,
                b"D" => ChangeKind::Deleted,
                b"U" => return Err(CaptureError::Unmerged(path)),
                _ => return Err(CaptureError::Malformed("unknown name-status letter")),
            };
            paths.insert(path, change);
        }
        Ok(paths)
    }

    fn untracked(&self) -> Result<Vec<String>, CaptureError> {
        let out = self.run(&["ls-files", "-z", "--others", "--exclude-standard", "--"])?;
        out.split(|b| *b == 0)
            .filter(|t| !t.is_empty())
            .map(path_text)
            .collect()
    }

    fn blob(
        &self,
        commit: &CommitId,
        path: &str,
        remaining: u64,
        limits: &Limits,
    ) -> Result<Vec<u8>, CaptureError> {
        let object = format!("{commit}:{path}");
        let size_text = self.run(&["cat-file", "-s", &object])?;
        let size: u64 = text(&size_text, "object size")?
            .trim()
            .parse()
            .map_err(|_| CaptureError::Malformed("cat-file -s did not print a size"))?;
        let too_large = |size| CaptureError::TooLarge {
            path: path.to_owned(),
            size,
            max: limits.max_total_bytes,
        };
        if size > remaining {
            return Err(too_large(size));
        }
        let bytes = self.run(&["cat-file", "blob", &object])?;
        if bytes.len() as u64 > remaining {
            return Err(too_large(bytes.len() as u64));
        }
        Ok(bytes)
    }

    /// The textual patch under pinned options; untracked files never appear
    /// in it because producing them would require touching the index.
    fn patch(
        &self,
        base: &CommitId,
        candidate: Option<&CommitId>,
    ) -> Result<Vec<u8>, CaptureError> {
        let mut args = vec![
            "diff",
            "--binary",
            "--full-index",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "-U3",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            base.as_str(),
        ];
        if let Some(candidate) = candidate {
            args.push(candidate.as_str());
        }
        args.push("--");
        self.run(&args)
    }

    fn root_commits(&self, commit: &CommitId) -> Result<Vec<CommitId>, CaptureError> {
        let out = self.run(&["rev-list", "--max-parents=0", commit.as_str(), "--"])?;
        let mut roots: Vec<CommitId> = text(&out, "root commits")?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                CommitId::new(l.trim())
                    .map_err(|_| CaptureError::Malformed("rev-list did not print commit ids"))
            })
            .collect::<Result<_, _>>()?;
        roots.sort();
        roots.dedup();
        Ok(roots)
    }
}

fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>, CaptureError> {
    let mut command = Command::new("git");
    command.arg("--no-optional-locks").arg("-C").arg(dir);
    for setting in PINNED_CONFIG {
        command.arg("-c").arg(setting);
    }
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().map_err(CaptureError::GitUnavailable)?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if stderr.len() > MAX_STDERR {
        let mut end = MAX_STDERR;
        while !stderr.is_char_boundary(end) {
            end -= 1;
        }
        stderr.truncate(end);
    }
    Err(CaptureError::Git {
        command: format!("git {}", args.join(" ")),
        status: output.status.code(),
        stderr,
    })
}

fn text<'a>(bytes: &'a [u8], what: &'static str) -> Result<&'a str, CaptureError> {
    std::str::from_utf8(bytes).map_err(|_| CaptureError::Malformed(what))
}

fn path_text(bytes: &[u8]) -> Result<String, CaptureError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| CaptureError::Malformed("a path is not UTF-8"))
}
