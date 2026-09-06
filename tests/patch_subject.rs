//! Exact local patch subjects captured from real Git repositories.
//!
//! Every test creates a disposable repository with the installed `git`,
//! commits with fixed author, committer, and dates so that commit ids are the
//! same on every platform, and reads it back through the adapter.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use checkspan::adapters::code::{CaptureError, Captured, Limits, Selection, Selector, capture};
use checkspan::contracts::{
    CandidateKind, Change, ChangeKind, CommitId, Digest, PatchError, PatchResult, Record,
    Timestamp, parse_record,
};
use checkspan::digests::{artifact_digest, patch_subject_digest};
use checkspan::validation::{self, ValidatedRecord};

const FIXED_ENV: &[(&str, &str)] = &[
    ("GIT_AUTHOR_NAME", "Checkspan Fixture"),
    ("GIT_AUTHOR_EMAIL", "fixture@checkspan.invalid"),
    ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z"),
    ("GIT_COMMITTER_NAME", "Checkspan Fixture"),
    ("GIT_COMMITTER_EMAIL", "fixture@checkspan.invalid"),
    ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z"),
];

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("checkspan-patch-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git_bytes(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .envs(FIXED_ENV.iter().copied())
        .output()
        .expect("git is installed");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn git(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git_bytes(root, args)).unwrap()
}

/// A fresh repository whose local configuration pins everything that could
/// make bytes differ between machines.
fn repo(name: &str) -> PathBuf {
    let root = temp_dir(name);
    git(&root, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Checkspan Fixture"),
        ("user.email", "fixture@checkspan.invalid"),
        ("core.autocrlf", "false"),
        ("core.safecrlf", "false"),
        ("commit.gpgsign", "false"),
    ] {
        git(&root, &["config", key, value]);
    }
    root
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn commit(root: &Path, message: &str) -> CommitId {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
    CommitId::new(git(root, &["rev-parse", "HEAD"]).trim()).unwrap()
}

fn now() -> Timestamp {
    Timestamp::new("2026-09-06T12:00:00Z").unwrap()
}

fn working_tree(root: &Path, base: Option<&CommitId>) -> Captured {
    let selection = Selection {
        candidate: Selector::WorkingTree,
        base: base.map(|b| b.as_str().to_owned()),
    };
    capture(root, &selection, &Limits::default(), &now()).unwrap()
}

fn commit_candidate(root: &Path, rev: &str, base: Option<&CommitId>) -> Captured {
    let selection = Selection {
        candidate: Selector::Commit(rev.to_owned()),
        base: base.map(|b| b.as_str().to_owned()),
    };
    capture(root, &selection, &Limits::default(), &now()).unwrap()
}

fn change<'a>(captured: &'a Captured, path: &str) -> &'a Change {
    captured
        .result
        .changes
        .iter()
        .find(|c| c.path == path)
        .unwrap_or_else(|| panic!("{path} is not among {:?}", captured.result.changes))
}

fn paths(captured: &Captured) -> Vec<&str> {
    captured
        .result
        .changes
        .iter()
        .map(|c| c.path.as_str())
        .collect()
}

fn assert_content(captured: &Captured, path: &str, kind: ChangeKind, bytes: &[u8]) {
    let change = change(captured, path);
    assert_eq!(change.change, kind, "{path}");
    assert_eq!(
        change.content_digest,
        Some(artifact_digest(bytes)),
        "{path}"
    );
    assert_eq!(change.size, Some(bytes.len() as u64), "{path}");
}

#[test]
fn a_clean_tree_is_captured_as_its_head_commit() {
    let root = repo("clean");
    write(&root, "README.md", b"# fixture\n");
    let base = commit(&root, "base");
    write(&root, "README.md", b"# fixture\nmore\n");
    write(&root, "src/lib.rs", b"pub fn one() -> u32 {\n    1\n}\n");
    let head = commit(&root, "candidate");

    let captured = working_tree(&root, Some(&base));
    let result = &captured.result;
    assert_eq!(result.candidate.kind, CandidateKind::Commit);
    assert_eq!(result.candidate.commit, head);
    assert_eq!(result.base_commit, base);
    assert_eq!(result.repository.root_commits, vec![base.clone()]);
    assert_eq!(paths(&captured), ["README.md", "src/lib.rs"]);
    assert_content(
        &captured,
        "README.md",
        ChangeKind::Modified,
        b"# fixture\nmore\n",
    );
    assert_content(
        &captured,
        "src/lib.rs",
        ChangeKind::Added,
        b"pub fn one() -> u32 {\n    1\n}\n",
    );
    assert_eq!(result.tool.name, "git");
    assert!(!result.tool.version.is_empty());
    assert!(result.check_bindings().is_empty());
    assert_eq!(
        captured.subject_digest,
        patch_subject_digest(result).unwrap()
    );

    // Naming the commit directly, with the base defaulting to its parent,
    // identifies the same subject.
    let by_commit = commit_candidate(&root, "HEAD", None);
    assert_eq!(by_commit.result.base_commit, base);
    assert_eq!(by_commit.result, *result);
    assert_eq!(by_commit.subject_digest, captured.subject_digest);

    // Reading it again changes nothing.
    let again = working_tree(&root, Some(&base));
    assert_eq!(again.result, *result);
    assert_eq!(again.subject_digest, captured.subject_digest);

    // A capture with no base compares the clean tree with itself.
    let trivial = working_tree(&root, None);
    assert_eq!(trivial.result.base_commit, head);
    assert!(trivial.result.changes.is_empty());
    assert_ne!(trivial.subject_digest, captured.subject_digest);
}

#[test]
fn a_dirty_tree_cannot_masquerade_as_its_commit() {
    let root = repo("dirty");
    write(&root, "README.md", b"# fixture\n");
    write(&root, "src/lib.rs", b"pub fn one() -> u32 {\n    1\n}\n");
    let head = commit(&root, "base");
    let clean = working_tree(&root, None);
    assert_eq!(clean.result.candidate.kind, CandidateKind::Commit);
    let committed = commit_candidate(&root, "HEAD", Some(&head));
    assert_eq!(committed.subject_digest, clean.subject_digest);

    // An unstaged edit.
    let edited = b"pub fn one() -> u32 {\n    2\n}\n";
    write(&root, "src/lib.rs", edited);
    let index_before = fs::read(root.join(".git/index")).unwrap();
    let count_before = git(&root, &["rev-list", "--count", "--all"]);
    let dirty = working_tree(&root, None);
    assert_eq!(dirty.result.candidate.kind, CandidateKind::WorkingTree);
    assert_eq!(dirty.result.candidate.commit, head);
    assert_eq!(dirty.result.base_commit, head);
    assert_eq!(paths(&dirty), ["src/lib.rs"]);
    assert_content(&dirty, "src/lib.rs", ChangeKind::Modified, edited);
    assert_ne!(dirty.subject_digest, clean.subject_digest);
    assert_ne!(dirty.subject_digest, committed.subject_digest);
    assert_ne!(
        dirty.result.candidate.patch_digest,
        clean.result.candidate.patch_digest
    );

    // Capturing committed nothing, staged nothing, stashed nothing, and did
    // not rewrite the index.
    assert_eq!(git(&root, &["rev-parse", "HEAD"]).trim(), head.as_str());
    assert_eq!(git(&root, &["rev-list", "--count", "--all"]), count_before);
    assert_eq!(git(&root, &["stash", "list"]), "");
    assert_eq!(git(&root, &["diff", "--cached", "--name-only"]), "");
    assert_eq!(git(&root, &["diff", "--name-only"]).trim(), "src/lib.rs");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index_before);

    // An untracked file alone makes the tree dirty.
    git(&root, &["checkout", "--", "src/lib.rs"]);
    write(&root, "notes.txt", b"scratch\n");
    let untracked = working_tree(&root, None);
    assert_eq!(untracked.result.candidate.kind, CandidateKind::WorkingTree);
    assert_eq!(paths(&untracked), ["notes.txt"]);
    assert_content(&untracked, "notes.txt", ChangeKind::Untracked, b"scratch\n");
    assert_ne!(untracked.subject_digest, clean.subject_digest);
    // The Git patch cannot express an untracked file; the subject does.
    assert_eq!(
        untracked.result.candidate.patch_digest,
        clean.result.candidate.patch_digest
    );

    // A staged file alone makes the tree dirty and is an addition.
    git(&root, &["add", "notes.txt"]);
    let staged = working_tree(&root, None);
    assert_eq!(staged.result.candidate.kind, CandidateKind::WorkingTree);
    assert_content(&staged, "notes.txt", ChangeKind::Added, b"scratch\n");
    assert_ne!(staged.subject_digest, clean.subject_digest);
    assert_ne!(staged.subject_digest, untracked.subject_digest);
}

#[test]
fn mutating_the_candidate_changes_the_subject_and_restoring_it_restores_the_digest() {
    let root = repo("mutate");
    write(&root, "a.txt", b"one\n");
    let base = commit(&root, "base");

    write(&root, "a.txt", b"two\n");
    let first = working_tree(&root, None);
    write(&root, "a.txt", b"three\n");
    let second = working_tree(&root, None);
    assert_ne!(second.subject_digest, first.subject_digest);
    assert_ne!(
        second.result.candidate.patch_digest,
        first.result.candidate.patch_digest
    );
    assert_ne!(
        change(&second, "a.txt").content_digest,
        change(&first, "a.txt").content_digest
    );

    write(&root, "a.txt", b"two\n");
    let restored = working_tree(&root, None);
    assert_eq!(restored.result, first.result);
    assert_eq!(restored.subject_digest, first.subject_digest);

    // Committing the same bytes is a different kind of candidate with the
    // same content digests.
    let head = commit(&root, "two");
    let committed = commit_candidate(&root, "HEAD", Some(&base));
    assert_eq!(committed.result.candidate.kind, CandidateKind::Commit);
    assert_eq!(committed.result.candidate.commit, head);
    assert_eq!(
        change(&committed, "a.txt").content_digest,
        change(&first, "a.txt").content_digest
    );
    assert_eq!(
        committed.result.candidate.patch_digest,
        first.result.candidate.patch_digest
    );
    assert_ne!(committed.subject_digest, first.subject_digest);
}

#[test]
fn paths_with_spaces_and_unicode_are_captured_exactly_and_identically_everywhere() {
    let root = repo("unicode");
    let tracked = "dir with spaces/héllo wörld ✓.txt";
    let untracked = "données/résumé – draft.md";
    write(&root, tracked, b"one\n");
    write(&root, "README.md", b"# fixture\n");
    let base = commit(&root, "base");

    write(&root, tracked, b"two\n");
    write(&root, untracked, b"# brouillon\n");
    let dirty = working_tree(&root, None);
    assert_eq!(dirty.result.candidate.kind, CandidateKind::WorkingTree);
    assert_eq!(paths(&dirty), [tracked, untracked]);
    assert_content(&dirty, tracked, ChangeKind::Modified, b"two\n");
    assert_content(&dirty, untracked, ChangeKind::Untracked, b"# brouillon\n");

    let head = commit(&root, "unicode");
    let committed = commit_candidate(&root, "HEAD", Some(&base));
    assert_eq!(committed.result.candidate.commit, head);
    assert_eq!(paths(&committed), [tracked, untracked]);
    assert_content(&committed, tracked, ChangeKind::Modified, b"two\n");
    assert_content(&committed, untracked, ChangeKind::Added, b"# brouillon\n");

    // Fixed author, committer, dates, and bytes make the commit ids, and
    // therefore the subject digests, the same on Windows and Linux.
    assert_eq!(
        [
            base.as_str(),
            head.as_str(),
            dirty.subject_digest.as_str(),
            committed.subject_digest.as_str(),
        ],
        [
            EXPECTED_BASE_COMMIT,
            EXPECTED_HEAD_COMMIT,
            EXPECTED_DIRTY_SUBJECT,
            EXPECTED_COMMITTED_SUBJECT,
        ]
    );
}

const EXPECTED_BASE_COMMIT: &str = "04f8062d9c2896bc5cf3e1e03dd6054b3d6bc140";
const EXPECTED_HEAD_COMMIT: &str = "017df6cb6b02c1fd06dc0286afa0cbdadd6e5360";
const EXPECTED_DIRTY_SUBJECT: &str =
    "sha256:bacd526282a0f936f40e57394665ce08566f90ad7d7187edb373281828c01f04";
const EXPECTED_COMMITTED_SUBJECT: &str =
    "sha256:e3110b8ed2731b3c6be49c30aa49942e91b71332a037a0a7b65bac1e77bb80df";

#[test]
fn deleted_and_ignored_paths_are_handled() {
    let root = repo("deleted");
    write(&root, ".gitignore", b"*.log\n");
    write(&root, "a.txt", b"a\n");
    write(&root, "b.txt", b"b\n");
    let head = commit(&root, "base");

    fs::remove_file(root.join("a.txt")).unwrap();
    write(&root, "build.log", b"noise\n");
    write(&root, "c.txt", b"c\n");
    let captured = working_tree(&root, None);
    assert_eq!(captured.result.base_commit, head);
    assert_eq!(paths(&captured), ["a.txt", "c.txt"]);
    let deleted = change(&captured, "a.txt");
    assert_eq!(deleted.change, ChangeKind::Deleted);
    assert_eq!(deleted.content_digest, None);
    assert_eq!(deleted.size, None);
    assert_content(&captured, "c.txt", ChangeKind::Untracked, b"c\n");

    // An ignored file is not part of the candidate and cannot move its digest.
    fs::remove_file(root.join("build.log")).unwrap();
    let without_noise = working_tree(&root, None);
    assert_eq!(without_noise.subject_digest, captured.subject_digest);

    // A deletion committed on top is a commit candidate with the same change.
    let next = commit(&root, "delete a");
    let committed = commit_candidate(&root, next.as_str(), Some(&head));
    assert_eq!(paths(&committed), ["a.txt", "c.txt"]);
    assert_eq!(change(&committed, "a.txt").change, ChangeKind::Deleted);
    assert_eq!(change(&committed, "c.txt").change, ChangeKind::Added);
}

#[test]
fn records_validate_against_the_bundled_schema_and_round_trip() {
    let root = repo("record");
    write(&root, "src/main.rs", b"fn main() {}\n");
    commit(&root, "base");
    write(
        &root,
        "src/main.rs",
        b"fn main() {\n    println!(\"hi\");\n}\n",
    );
    write(&root, "docs/notes — draft.md", b"notes\n");
    let captured = working_tree(&root, None);
    let result = &captured.result;

    let text = serde_json::to_string_pretty(result).unwrap();
    assert_eq!(
        common::schema_errors(PatchResult::SCHEMA_ID, &text),
        Vec::<String>::new()
    );
    let parsed: PatchResult = parse_record(&text).unwrap();
    assert_eq!(&parsed, result);
    assert_eq!(
        patch_subject_digest(&parsed).unwrap(),
        captured.subject_digest
    );

    let report = validation::validate(text.as_bytes(), &validation::Limits::default());
    assert!(report.is_valid(), "{:?}", report.diagnostics);
    assert!(matches!(
        report.record,
        Some(ValidatedRecord::PatchResult(ref p)) if p == result
    ));

    // Contract checks refuse what the schema alone cannot.
    let digest =
        Digest::new("sha256:0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap();
    let mut broken = result.clone();
    broken.changes.push(Change {
        path: "zzz".into(),
        change: ChangeKind::Deleted,
        content_digest: Some(digest.clone()),
        size: Some(1),
    });
    broken.changes.push(Change {
        path: "zzz".into(),
        change: ChangeKind::Added,
        content_digest: None,
        size: None,
    });
    broken.changes.insert(
        0,
        Change {
            path: "../escape".into(),
            change: ChangeKind::Untracked,
            content_digest: Some(digest),
            size: Some(0),
        },
    );
    let errors = broken.check_bindings();
    assert!(errors.contains(&PatchError::DeletedWithContent("zzz".into())));
    assert!(errors.contains(&PatchError::UnsortedPaths("zzz".into())));
    assert!(errors.contains(&PatchError::MissingContent("zzz".into())));
    assert!(errors.contains(&PatchError::PathNotRelative("../escape".into())));
    let broken_text = serde_json::to_string(&broken).unwrap();
    let report = validation::validate(broken_text.as_bytes(), &validation::Limits::default());
    assert!(!report.is_valid());
    assert!(report.record.is_none());
}

/// Environment variable naming the directory a child process must find to
/// be outside any repository. Git discovery walks upward, and a developer's
/// home directory may itself be a repository, so the child bounds discovery
/// with `GIT_CEILING_DIRECTORIES`; a test cannot set process environment
/// for its siblings.
const NOT_A_REPO_ENV: &str = "CHECKSPAN_PATCH_NOT_A_REPO";

#[test]
fn not_a_repository_child() {
    let Ok(dir) = std::env::var(NOT_A_REPO_ENV) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let selection = Selection {
        candidate: Selector::WorkingTree,
        base: None,
    };
    let error = capture(&dir, &selection, &Limits::default(), &now()).unwrap_err();
    assert!(
        matches!(error, CaptureError::NotARepository(ref p) if *p == dir),
        "{error}"
    );
}

#[test]
fn limits_and_errors_are_explicit() {
    let plain = temp_dir("not-a-repo");
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["not_a_repository_child", "--exact", "--nocapture"])
        .env(NOT_A_REPO_ENV, &plain)
        .env("GIT_CEILING_DIRECTORIES", plain.parent().unwrap())
        .status()
        .unwrap();
    assert!(status.success(), "not-a-repository child failed");
    let selection = Selection {
        candidate: Selector::WorkingTree,
        base: None,
    };

    let root = repo("errors");
    write(&root, "a.txt", b"a\n");
    commit(&root, "only");
    let error = capture(
        &root,
        &Selection {
            candidate: Selector::Commit("does-not-exist".into()),
            base: None,
        },
        &Limits::default(),
        &now(),
    )
    .unwrap_err();
    assert!(
        matches!(error, CaptureError::UnknownRevision(ref r) if r == "does-not-exist"),
        "{error}"
    );

    let error = capture(
        &root,
        &Selection {
            candidate: Selector::Commit("HEAD".into()),
            base: None,
        },
        &Limits::default(),
        &now(),
    )
    .unwrap_err();
    assert!(matches!(error, CaptureError::NoParent(_)), "{error}");

    write(&root, "b.txt", b"bb\n");
    write(&root, "c.txt", b"ccc\n");
    let tight = Limits {
        max_paths: 1,
        ..Limits::default()
    };
    let error = capture(&root, &selection, &tight, &now()).unwrap_err();
    assert!(
        matches!(error, CaptureError::TooManyPaths { count: 2, max: 1 }),
        "{error}"
    );

    let small = Limits {
        max_total_bytes: 5,
        ..Limits::default()
    };
    let error = capture(&root, &selection, &small, &now()).unwrap_err();
    assert!(
        matches!(error, CaptureError::TooLarge { ref path, size: 4, max: 5 } if path == "c.txt"),
        "{error}"
    );

    // The same ceiling applies when content is read from a commit.
    commit(&root, "more");
    let error = capture(
        &root,
        &Selection {
            candidate: Selector::Commit("HEAD".into()),
            base: None,
        },
        &small,
        &now(),
    )
    .unwrap_err();
    assert!(
        matches!(error, CaptureError::TooLarge { ref path, size: 4, max: 5 } if path == "c.txt"),
        "{error}"
    );
}
