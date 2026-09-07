# Checkspan — development log

## Execution state

- Canonical plan: [CHECKSPAN-PSPR](../PLANNING/CHECKSPAN-PSPR.md), Draft 0.2.
- Product: Checkspan, an independent exploration.
- Repository: [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan).
- Implementation authorization: **full STS approved by Basho on 2026-09-06** ("Approved for full STS now"). Execution proceeds sequentially and halts at every explicit stop in PSPR §0.3; the first stop is the M1 boundary after CS-07.
- Implementation branch: `codex/checkspan-m1`. Per Basho's instruction of 2026-09-06, every completed prompt is committed on that branch, fast-forwarded into `main`, and both refs are pushed.
- M2 authorization: **"Run M2 STS" received from Basho on 2026-09-06** after the M1 report; CS-08 through CS-14 are authorized, with the M2 boundary (after CS-14) as the next explicit stop. Basho reiterated: commit and merge to `main` after every prompt.
- M3 authorization: **"Run M3 STS now please" received from Basho on 2026-09-06** after the M2 report. CS-15 through CS-24, CS-R01 through CS-R12, and CS-25 are authorized in order. Stops that remain inside M3 because the approval named no signer, corpus, model, or budget: first use of Basho's signing identity (CS-22), admission of the pilot corpus (CS-R02), first model endpoint and its egress and usage budget (CS-R05), the evaluation usage budget (CS-R11), and the M3 boundary after CS-25.
- Current prompt: **CS-18 complete; CS-19 next.**
- Session handoff: see [CHECKSPAN-HANDOFF.md](../CHECKSPAN-HANDOFF.md) at the project root.
- Implemented product behavior: `checkspan validate <file>` and `checkspan inspect <file>` over any supported record (now including `patch_result`), with graph admission (dependency resolution, port and type compatibility, cycles, hidden proof dependencies, external imports, gate wait chains, target closure, deterministic order) for graph documents; stable JSON output and exit codes; no dispatch, run store, or writes. Library code additionally provides the durable run ledger (M2) and, from CS-15, capture of exact local patch subjects from a Git repository.

## DOC-00 — Name, repository wiring, and review draft

**Date:** 2026-09-06.  
**Scope authorized:** choose a new independent product name, connect the provided repository, and draft a granular PSPR for review.  
**Status:** complete; exploration papers and draft PSPR published to main.  
**Documentation commit:** 6bc2bb888147123c4053fff0bf4a05bf9354cf1f

**Changes:** introduced Checkspan and its standalone positioning; prepared the current one-pager, schema notes, and README; retained the original v0.1 papers unchanged; drafted the 36-prompt, five-milestone PSPR and verification ledger. Added basic text/ignore conventions for the new repository.

**Verification:** original document hashes checked before writing; current document read-back and internal links checked; prompt IDs, ordering, per-prompt objectives/gates, milestone ranges, and unapproved state reviewed. GitHub metadata identified an empty public repository with default branch `main` before wiring.

**Not performed:** no Cargo package, runtime, worker, verifier, integration, CI workflow, credential setup, or release. No implementation/live gate is marked passed.

**Working-copy discipline:** use the user-named folder directly. No secondary checkout/worktree or dependency tree is required. Temporary drafting payloads remain outside the repository and are not product artifacts.

## Prompt entry format

For every approved prompt, append: prompt ID and approved scope; before/source SHA; changed paths; verification commands and outcomes; native/hosted/operator evidence references; acceptance status; implementation commit; remote SHA; open blockers; storage/worktree closeout.

Do not replace historical failures with a later success. Record retries and superseding evidence separately. Keep prompt acceptance separate from release authorization.

## DOC-01 - Publication closeout

The initial documentation commit 6bc2bb888147123c4053fff0bf4a05bf9354cf1f was pushed to origin/main and the remote SHA was verified with git ls-remote. Staged diff checking and the configured no-slop commit/push hooks passed. The 36-prompt roster and all required prompt fields were checked; internal document links resolve.

The only retained worktree is the canonical working folder on main, used for this exploration and future approved work. No secondary worktree, build output, dependency tree, or unpublished implementation exists. Temporary drafting payloads remain outside the repository; no deletion was performed.

CS-01 through CS-36 remain not approved and not started. This closeout changes documentation only. Review and explicit STS approval are still required.

## DOC-02 — Correct the omitted multi-agent RAG scope

**Date:** 2026-09-06.  
**Source commit:** df18707d0ae5cea46d8193b670ec468eedfa2650.  
**Review trigger:** Basho noted that the PSPR did not mention multi-agent RAG.  
**Scope:** documentation revision only; no implementation or model/corpus access approval.

Draft 0.1 parked RAG ingestion, model execution, and parallel workers. Draft 0.2 corrects that omission: a proposed combined M3 pilot now includes distinct requirements/implementation retrieval agents, synthesis, challenge, mechanical evidence checking, and explicit human semantic assessment. First-pilot placement is the draft recommendation; no unanswered scope question is treated as approval.

**Changed paths:** PLANNING/CHECKSPAN-PSPR.md; CHECKSPAN-ONE-PAGER.md; CHECKSPAN-SCHEMA-NOTES.md; README.md; docs/CHECKSPAN-DEVLOG.md; docs/CHECKSPAN-VERIFICATION.md.

**Roster:** original CS-01–CS-36 identifiers preserved; CS-R01–CS-R12 inserted after CS-24 and before CS-25, giving 48 prompts and five milestone approval cuts. Added V9–V11 and 12 RAG review cases. Active product/schema papers are v0.3. Historical v0.1 files remain unchanged.

**Document gate:** exact 48-ID sequence and five required fields per prompt; dependency ordering; active-document links and version/scope consistency; Markdown whitespace and historical hashes. Product gates remain not run. This entry's documentation commit is identified by Git history; final publication SHA is reported separately after remote verification.

**Execution state:** all implementation prompts remain unapproved and unstarted. No model runtime, corpus, credential, spend, agent worker, generated build tree, or extra worktree was introduced. Temporary drafting data remains outside the repository; no deletion is authorized or performed.

## CS-01 — Establish the reproducible Rust baseline

**Date:** 2026-09-06.  
**Approved scope:** full STS (Basho, 2026-09-06); CS-01 is the first prompt of M1.  
**Source SHA before work:** 8de894f65863e8ff95d1dad0d9c2415e7d606596 (main).  
**Branch:** `codex/checkspan-m1`.

**Changed paths:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, `src/lib.rs`, `src/cli.rs`, `src/main.rs`, `tests/cli_smoke.rs`, `.github/workflows/ci.yml`, `docs/CHECKSPAN-DEPENDENCIES.md`, this log, the verification ledger.

**Prerequisites verified (V0):** rustc/cargo 1.98.0 stable on `x86_64-pc-windows-msvc` with rustfmt and clippy; MSVC linker confirmed by a scratch `cargo build` outside the repository; git 2.54.0; gh 2.92.0 authenticated as USS-Parks; 279 GB free on the working volume; cargo-deny 0.19.7 and cargo-audit 0.22.1 present. Clean tree on `main` before branching.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo generate-lockfile` then `cargo build --locked` | passed; 22 crates |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed after scoping `missing_docs` to the library crate (first run failed on the binary and test crates) |
| `cargo test --locked` | passed, 3 tests: `--version` output, `--help` output, unknown-flag exit code 2 |
| `cargo deny check` | advisories, bans, licenses, sources ok (own crate's pending license ignored via `[licenses.private]`) |
| `cargo audit` | no advisories against 22 dependencies |

**Not claimed:** hosted CI behavior. The workflow definition was reviewed and its four commands were run locally; the hosted matrix result is recorded at CS-07 after the branch is pushed.

**Acceptance:** CS-01 gate passed locally. Implementation commit SHA is recorded in the CS-02 entry.  
**Open blockers:** none.  
**Storage/worktree:** canonical checkout only; `target/` generated locally and ignored.

## CS-02 — Define identities and graph envelopes

**Date:** 2026-09-06.  
**Source SHA before work:** 09302940b5ab9612ed2cd3d25738589d0fe9db43 (CS-01 implementation commit, `codex/checkspan-m1`).  
**CS-01 hosted evidence:** GitHub Actions run 34056847082 on that SHA, `ubuntu-24.04` and `windows-2025`, conclusion success (recorded here as `hosted_ci` for CS-01; M1 hosted acceptance is still CS-07).  

**Changed paths:** `schemas/v1/{common,node-ref,graph-ref,node-spec,graph-spec,graph-run}.schema.json`; `src/contracts/{mod,ids,record,registry,graph}.rs`; `src/lib.rs`; `tests/contract_identity.rs`; `tests/fixtures/contracts/{valid,invalid}/*.json` (4 valid, 20 invalid); `Cargo.toml`, `Cargo.lock`; `docs/CHECKSPAN-DEPENDENCIES.md`; this log; the verification ledger.

**Design as implemented:** every top-level record is self-describing through `record` and `schema_version`; parsing is header-first, so an unsupported version is reported before any shape error. Identifiers share one grammar (`^[a-z0-9][a-z0-9_.-]{0,127}$`); revisions start at 1; display names are optional and never identity. `NodeRef` is `{graph_id, node_id, revision}` and equality covers all three. `GraphSpec` targets are full `NodeRef`s that must resolve to a node in the same graph at its exact revision; `supersedes` must name an earlier revision of the same graph. `GraphRun` binds `run_id` to an exact `GraphRef` and may name an earlier run as `budget_lineage_ref`, never itself. Schema `$id`s live under the reserved `checkspan.invalid` host and are served only by the bundled registry.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed (after one `cargo fmt --all`) |
| `cargo clippy --locked --all-targets -- -D warnings` | passed (one `type_complexity` finding fixed with a type alias) |
| `cargo test --locked` | passed: 3 unit (identifier grammar, revision floor, timestamp shape), 3 CLI smoke, 14 contract-identity tests |
| `cargo deny check` | advisories, bans, licenses, sources ok |
| `cargo audit` | no advisories against 95 lockfile entries |

**Gate evidence (V1/V2):** same local slug distinguished across graphs and revisions (`HashSet` of three refs, Display output); two runs of one graph revision distinct; display-name change leaves `GraphRef`/`NodeRef` equal; `schema_version: 2` with an extra field rejects as `UnsupportedVersion`, not a shape error; wrong/unknown/missing record headers reject; missing, empty, foreign, unknown, revision-mismatched, and duplicate targets reject; `supersedes` foreign/not-earlier reject; self-lineage rejects; all six bundled schemas are valid Draft 2020-12 and compile offline; `$ref`s to unbundled `https://`, `file://`, and unknown in-namespace IDs fail to compile rather than fetch. Each valid fixture passes schema validation, typed parse, identity check, and a serialize/parse round trip.

**Not claimed:** CLI exposure of validation (CS-06), bounded parsing and duplicate-key detection (CS-05), node contract bodies (CS-03), hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-02 gate passed locally. Implementation commit SHA is recorded in the CS-03 entry.  
**Open blockers:** none.

## CS-03 — Define immutable node and evidence contracts

**Date:** 2026-09-06.  
**Source SHA before work:** 918a8431ec8d46b19029e3ecbbb77e785b25716c (CS-02 implementation commit; also `main` after the fast-forward merge Basho requested).  

**Changed paths:** `schemas/v1/common.schema.json` (adds `version`, `digest`, `policy_ref`, `type_ref`, `verifier_ref`); `schemas/v1/node-spec.schema.json` (full contract with kind-conditional rules); `schemas/v1/evidence-ref.schema.json` (new); `src/contracts/ids.rs` (adds `Ident`, `Version`, `Digest`, `Exactly<V>`); `src/contracts/node.rs`, `src/contracts/evidence.rs` (new); `src/contracts/{graph,mod,registry}.rs`; `tests/common/mod.rs` (shared helpers); `tests/node_contracts.rs` (new); `tests/contract_identity.rs`; fixtures regenerated with full node bodies (`contracts/` 4 valid + 20 invalid, `nodes/` 5 valid + 35 invalid, `evidence/` 2 valid + 6 invalid); this log; the verification ledger; the dependency record.

**Design as implemented:** `kind` is one of `task`, `check`, `human_gate`, `sink`. A `check` or `sink` needs at least one dependency; a `human_gate` needs exactly one `human` evidence port and no other kind may declare one. Dependencies name an exact `NodeRef`, an output port, and the expected `TypeRef` (`schema_id`, `version`, `digest`). Evidence ports declare `kind` (`rag`, `human`, `logs`, `code`, `proofs`), `expected_type`, `required`, a non-empty `allowed_source_scope`, and a `handling_policy_ref`; declaration does not enforce the boundary. `acceptance` pins `claim`, a non-empty `required_checks` list, the `verifier` (`id`, `version`, `digest`), and `policy_ref`. `retry_policy` is inline with a fixed format `version` of 1 (the `Exactly<1>` marker rejects any other value while parsing) and defaults to three attempts, all failure classes, gate on exhaustion. `authority_policy_ref` is mandatory on every node so authority is declared before dispatch. `resource_scope` lists `exclusive`/`shared` claims. `supersedes` must name an earlier revision of the same node. `EvidenceRef` binds `port_name`, `locator`, `subject`, `version`, `content_digest`, `provenance` (`producer`, optional `attestation_ref`), `policy_ref`, and optional `valid_until`. No record or schema property can express a status, verdict, or acceptance.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 29 tests (3 unit, 3 CLI smoke, 14 contract identity, 9 node/evidence contract) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-02 |

**Gate evidence (V1/V2):** mandatory checks and inputs cannot disappear (empty or missing `required_checks`, missing `verifier`, an extra `skip_checks` field, missing `result_type`, missing port `required` or `allowed_source_scope`, empty scope, missing `authority_policy_ref` all reject); wrong port/output types reject (unknown node kind, non-identifier port name, missing `expected_type`, unknown port kind, malformed digests on verifier and result type, version 0, unknown resource access); unsupported policy versions reject (`retry_policy.version: 2` fails with "not supported; this build supports 1", zero `max_attempts`, unknown failure class and exhaustion routing, policy ref version 0); kind-conditional rules are enforced by both the schema (`if`/`then`, `contains` with `minContains`/`maxContains`) and the Rust check; rules the schema cannot express (duplicate dependency, port name, resource; `supersedes` on another node or a later revision; whitespace-only prompt) reject in the Rust check on schema-valid documents; every valid node fixture contains no outcome key, the node schema offers none, and a document carrying `status` rejects; retry defaults are explicit after parsing; evidence fixtures round-trip and malformed digest, timestamp, unknown field, and missing provenance reject.

**Not claimed:** cross-node type compatibility, cycles, and hidden proof dependencies (CS-06); attempts, receipts, and decisions (CS-04); actual evidence resolution or scope enforcement (CS-16); hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-03 gate passed locally. Implementation commit SHA is recorded in the CS-04 entry.  
**Open blockers:** none.

## CS-04 — Define attempts, receipts, and gate decision records

**Date:** 2026-09-06.  
**Source SHA before work:** abe323dfe93c910d3d175d4dd80f680a4743a947 (CS-03 implementation commit; also `main` after the fast-forward merge).  

**Changed paths:** `schemas/v1/{attempt,verifier-receipt,gate-packet,gate-decision,node-view}.schema.json` (new); `schemas/v1/common.schema.json` (adds `attempt_number`, `attempt_ref`, `receipt_ref`, `provenance_level`, `text`); `src/contracts/{attempt,gate,view}.rs` (new); `src/contracts/{ids,record,registry,mod}.rs`; `tests/outcome_contracts.rs` (new); `tests/fixtures/outcomes/` (28 valid, 67 invalid); this log; the verification ledger; the dependency record.

**Design as implemented:** `Attempt` records run, node, attempt number, owner, timestamps, an optional repair hint, the input manifest and dependency receipts frozen at dispatch, the result and produced evidence, an `execution` record, and the `verifier_receipt` reference once one exists; it has no acceptance field. `execution.outcome` is `completed`, `failed`, `timed_out`, or `cancelled`; `VerifierReceipt.verdict` is `accept`, `reject`, or `undecidable`; the two vocabularies are disjoint and each enum rejects the other's words. A receipt binds an exact `AttemptRef`, subject, result digest, context digest, verifier identity with digest, policy, verdict, reason fields, validity, and a `provenance` with level `local_controller`, `authenticated_upstream`, or `signed_operator`; `reject` and `undecidable` need a `reason_code`, `accept` cannot carry a `retry_hint`. `VerifierReceipt::binds(&Attempt)` requires the exact attempt, completed execution, and an equal result digest. `GatePacket` carries purpose (`resolve_work`, `assess_claims`, `authorize_action`), sealed context references, a `subject_digest`, the offered options (restricted per purpose), the exact action for authorization packets, the authority policy, and an expiry. `GateDecision` binds a packet by id and digest, names the authority and its authentication level, carries a typed decision (`retry`, `revise`, `cancel`, `approve_action`, `deny_action`, `record_assessment`), an assessment payload exactly for `record_assessment`, and a scope that must name the action for action decisions. `GateDecision::binds(&GatePacket)` checks id, digest, offered option, run, node, and action; `authorizes(action)` is true only for `approve_action` on that exact action; `is_denial()` covers `deny_action` and `cancel`. `NodeView` has status `open`, `running`, `accepted`, `rejected`, `failed`, `gated`, or `cancelled`; `accepted` and `rejected` must name the receipt that determined them and the receipt must be about the same node and run; no other status may carry one. All five are self-describing records parsed header-first with bundled Draft 2020-12 schemas that encode the same conditionals.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 42 tests (3 unit, 3 CLI smoke, 14 contract identity, 9 node/evidence, 13 outcome) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-02 |

**Gate evidence (V1/V2):** records cannot express acceptance without receipt bindings (attempt with a receipt but no result, or with failed/timed-out execution, or a receipt about another node or run; view `accepted`/`rejected` without a receipt or attempt, or with a receipt about another node or run; receipt on `open`/`running` views); an authentic denial remains a denial (the `deny_action` fixture is well-formed, binds to its packet, `is_denial()` is true, `authorizes()` is false for the exact action; `approve_action` authorizes only the exact action and target; `cancel` is a denial); a timeout cannot parse as a verdict (`timed_out`, `timeout`, `failed`, `completed` all fail as `verdict`; `reject`/`accept` fail as `execution.outcome`; `verdict`/`outcome` fields on the wrong record reject; the two enums' string sets are disjoint); a decision bound to a changed packet digest, a retargeted action, a packet that did not offer the decision, or another node fails to bind; `record_assessment` requires its payload and other decisions cannot carry one; packets offer only purpose-allowed options and name an action exactly for authorization; a decision record cannot be read as a receipt and no status vocabulary includes `approved`/`proved`. Golden fixtures cover all 4 execution outcomes, 3 verdicts, 3 provenance levels, 3 purposes, 6 decisions, 3 assessment outcomes, and 7 statuses, and the coverage is asserted.

**Not claimed:** any state transition (CS-10); receipt admission against a verifier identity or trust policy (CS-19); signature verification of decisions (CS-22); expiry against a real clock; hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-04 gate passed locally. Implementation commit SHA is recorded in the CS-05 entry.  
**Open blockers:** none.

## CS-05 — Implement bounded offline document validation

**Date:** 2026-09-06.  
**Source SHA before work:** 3eecbf06344080a4c15c409fa9ad5fa984fb4d26 (CS-04 implementation commit; also `main` after the fast-forward merge).  

**Changed paths:** `src/validation/{mod,parse,schema}.rs` (new); `src/lib.rs`; `src/contracts/{record,mod}.rs` (public `RecordHeader` / `read_header`); `Cargo.toml` (`jsonschema` promoted to a runtime dependency); `deny.toml` (allow `MIT-0`, `Zlib`); `tests/document_validation.rs` (new); this log; the verification ledger; the dependency record.

**Design as implemented:** `validate(bytes, &Limits)` runs fixed stages in order and stops at the first that fails: size (default 4 MiB), UTF-8, strict parse, header, schema, shape, contract. The strict parser is a serde visitor that builds a `serde_json::Value` while enforcing a nesting limit (default 64 containers), rejecting any repeated key inside one object, and rejecting trailing content; every parse failure carries the JSON pointer of the container or key involved. The header stage requires a JSON object with `record` and `schema_version` and matches them against the registry, listing the supported records on failure. The schema stage uses validators compiled once per process from the bundled corpus through an in-crate retriever that serves only bundled `$id`s; `jsonschema` is built with `default-features = false`, so no HTTP, file, or TLS resolver exists in the binary. A frozen keyword set (`SUPPORTED_KEYWORDS`) is enforced before compilation: any schema using another Draft 2020-12 keyword is refused. Schema diagnostics carry the instance JSON pointer; contract diagnostics carry the containing field (`/targets`, `/nodes/<i>`, `/status`, ...). A valid document yields a `ValidatedRecord` of the matching kind. Nothing in the module touches the filesystem or the network.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 62 tests (8 unit incl. 5 parser, 3 CLI smoke, 14 identity, 9 node/evidence, 13 outcome, 15 document validation) |
| `cargo deny check` | advisories, bans, licenses, sources ok (after allowing `MIT-0` and `Zlib`, which entered the runtime graph with `jsonschema`) |
| `cargo audit` | no advisories against 95 lockfile entries |

**Gate evidence (V1/V2):** oversized input rejects at the size stage with nothing interpreted; a 70-deep nesting inside an otherwise valid graph rejects at the depth stage with the path `/nodes/0/display_name/0/0...` and reaches the schema stage only when the limit is raised; duplicate keys reject with the repeated key's path (`/graph_id`, `/nodes/0/id`); empty, malformed, unterminated, trailing-content, and non-UTF-8 inputs reject at the syntax stage; non-object documents and missing or unsupported headers reject at the header stage with the supported list; schema diagnostics carry field paths (`/graph_id`, `/nodes/0`, `/verdict`); contract diagnostics name the containing field; every valid fixture of every record kind yields its typed record and every invalid top-level fixture is rejected by exactly one stage; `https://`, `http://`, `file://`, `../` traversal, unknown in-namespace, and missing-fragment references all fail to compile while a bundled reference compiles and validates; `Cargo.lock` contains no `reqwest`, `hyper`, `rustls`, `aws-lc-rs`, `tokio`, `ureq`, or `url`; the bundled corpus uses exactly the frozen keyword set and every frozen keyword has an inline positive and negative case (`type`, `properties`, `required`, `additionalProperties`, `enum`, `const`, `allOf`, `if`/`then`/`else`, `not`, `items`, `minItems`, `contains`/`minContains`/`maxContains`, `minimum`, `maximum`, `minLength`, `maxLength`, `pattern`, `format`, `$ref`/`$defs`); `$dynamicRef`, `unevaluatedProperties`, `dependentSchemas`, `patternProperties`, `$anchor`, `anyOf`, `oneOf`, `uniqueItems`, and `$comment` are refused before compilation while property names that merely look like keywords are not.

**Not claimed:** CLI exposure (`validate`/`inspect` are CS-06); cross-node graph admission (CS-06); hosted matrix for this commit (queued on push; recorded at CS-07).

**Acceptance:** CS-05 gate passed locally. Implementation commit SHA is recorded in the CS-06 entry.  
**Open blockers:** none.

## CS-06 — Implement graph admission and read-only commands

**Date:** 2026-09-06.  
**Source SHA before work:** f7faf4118649054f568aa62922eaf2ad187aaf2f (CS-05 implementation commit; also `main` after the fast-forward merge).  

**Changed paths:** `src/graph/mod.rs` (new); `src/cli.rs` (rewritten: `validate` and `inspect`); `src/lib.rs`; `src/validation/mod.rs` (adds `Stage::Admission`); `Cargo.toml` (`autoexamples = false`); `examples/README.md`, `examples/graphs/{review-pilot,research-pilot}.json`, `examples/records/graph-run.json`, `examples/invalid/*.json` (9); `tests/graph_admission.rs`, `tests/cli_commands.rs` (new); `tests/fixtures/graphs/` (4 valid, 17 invalid); this log; the verification ledger; the dependency record.

**Design as implemented:** `graph::admit(&GraphSpec)` composes the envelope identity rules and every node's contract rules with the cross-node rules: dependencies must name this graph (a foreign graph is an unsupported external import), an existing node, and that node's exact revision; a node cannot depend on itself; the only output port is `result`; a dependency's `expected_type` must equal the producer's `result_type` including digest; a `proofs` port names in-graph sources as `node:<id>`, each of which must be a declared dependency, and any other proof source is an unsupported import; a `human` port on a `human_gate` names the nodes it resolves as `node:<id>`, and a gate joined to a resolved node by an acceptance chain in either direction (or resolving itself) is a gate wait cycle. Cycles are found by Kahn's algorithm with document-order tie-breaking and reported once with a deterministic path. Admission returns every error in document order, or an `AdmittedGraph` with the topological `order`, the `required` closure of the targets, the `optional` remainder, and gate bindings. It reads the graph only. The CLI reads one file (checking size from metadata before reading), runs the validation pipeline, adds admission for graph documents, and prints one JSON object with `command`, `path`, `valid`, `record`, `schema_version`, `schema_id`, and staged, path-bearing `diagnostics`; `inspect` adds a `graph` description or an `identity` summary. Exit codes: 0 valid, 1 rejected, 2 usage, 3 unreadable file. Output keys are sorted, so output is byte-stable across runs.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 83 tests (8 unit, 3 CLI smoke, 9 CLI command, 14 identity, 15 document validation, 12 graph admission, 9 node/evidence, 13 outcome) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-05 |

**Gate evidence (V1/V2):** the chain-with-gate graph admits with order `cs_patch, cs_ci, cs_ci_gate, cs_packet`, required closure `cs_patch, cs_ci, cs_packet`, the gate optional and bound to `cs_ci`; the six-node diamond admits with both branches before the join and everything required; an isolated target admits with the unrelated side nodes reported optional; node revisions independent of the graph revision admit; admission is deterministic (equal results across runs); unresolved, foreign-graph, revision-mismatched, and self dependencies reject with the exact reference; an unknown output port and a mismatched or digest-drifted result type reject; a two-node and a three-node cycle each produce exactly one `Cycle` error with a deterministic path (`cs_patch -> cs_ci -> cs_patch`); hidden, unresolved, and external proof sources reject; a gate that depends on the node it resolves, a node that depends on its gate, a gate that resolves itself, and a gate that resolves an unknown node reject; identity and node-contract errors surface through admission in document order with `/nodes/<i>` paths. Through the binary: the three valid examples exit 0 with empty diagnostics; all nine invalid examples exit 1 with the expected single rejecting stage (`admission`, `duplicate_key`, `header`, `schema`) and a path on every diagnostic; an unreadable file exits 3 with an `io` diagnostic; `inspect` reports order, closure, gates, and per-node deps and checks, or `identity` for a run or receipt; `inspect` on an invalid document exits 1 without a `graph` section; running both commands in a scratch directory creates no files and leaves the input bytes unchanged, and no `.checkspan` directory appears in the repository; output is identical across two runs with empty stderr; no subcommand prints help and exits 2.

**Not claimed:** native Linux behaviour and the hosted matrix for this commit (queued on push; both recorded at CS-07); any dispatch, run store, or state transition (M2).

**Acceptance:** CS-06 gate passed locally. Implementation commit SHA is recorded in the CS-07 entry.  
**Open blockers:** none.

## CS-07 — Accept the contract-explorer milestone

**Date:** 2026-09-06.  
**Accepted source SHA:** ac639a4d68bc9e2553af26acc189d8d4c45a99b6 (CS-06 implementation commit; `codex/checkspan-m1` and `main`).  
**Scope:** reports and docs only. No product code changed; no code defect was found, so no implementation prompt was reopened.

**Changed paths:** `docs/CHECKSPAN-M1-GUIDE.md` (new beginner instructions); `README.md` (status and entry points); `test-evidence/checkspan/CS-07/{README.md,run-examples.sh,windows-x64.txt,linux-x64.txt,linux-x64-build.log}` (new); this log; the verification ledger; the dependency record.

**Native Windows x64 (local_native, passed):** release build of the accepted SHA from the clean canonical checkout, binary SHA-256 `5eef3b77…28add3`; `run-examples.sh` shows the 3 valid examples exit 0 through both commands, the 9 invalid examples exit 1 with their expected stage, a missing file exits 3, no arguments and an unknown flag exit 2. The V1 ladder (fmt, clippy `-D warnings`, 83 tests) passed on this SHA in the CS-06 entry.

**Native Linux x64 (local_native, passed):** WSL2 Ubuntu 26.04, kernel 6.6.87.2; fresh `git clone` of the public repository from GitHub, `git checkout ac639a4d…`, toolchain 1.98.0 installed by `rustup toolchain install` from the pinned file; `cargo fmt --check` passed, `cargo clippy --locked --all-targets -D warnings` passed, `cargo test --locked` passed all 83 tests, `cargo build --release --locked` produced binary SHA-256 `ed7b382b…bdcd9d`; `run-examples.sh` produced the same 27 result lines as Windows, with identical exit codes, stages, and output digests for every example. The evidence files contain no tokens or secrets (scanned).

**Hosted matrix (hosted_ci, passed):** GitHub Actions run 34059580357 on ac639a4d…, jobs `check (ubuntu-24.04)` 101557584686 and `check (windows-2025)` 101557584591, every step successful, all 8 test binaries (83 tests) passing on each runner; run 34059583565 on `main` at the same SHA also passed. Every earlier M1 commit passed the same matrix: 34056847082 (0930294), 34057533885 (918a843), 34058130081 (abe323d), 34058737149 (3eecbf0), 34059107159 (f7faf41).

**Gate (V0–V2):** correct checkout, branch, and SHA recorded; clean tree; the CLI-only instructions in the M1 guide and `examples/README.md` reproduce every expected status and exit code on both platforms; no worker, run store, credential path, or network client exists (the build contains no HTTP or TLS crate, and both commands are shown to create no files). Contract and graph gates (V2) are the committed test suites from CS-02–CS-06, passing natively on both platforms and on the hosted matrix.

**Not claimed:** anything beyond offline validation and admission. No execution, storage, verifier, retrieval, model, signature, GitHub-evidence, or packaging claim is made.

**Storage and worktree closeout (removal requires Basho's authorization; none performed):**

| Item | Location | Purpose | Retirement condition |
| --- | --- | --- | --- |
| Canonical checkout | `C:\Users\17076\Documents\Codex\Work Graph Project`, `codex/checkspan-m1` (= `main`) | Sequential implementation lane | Retained |
| Build output | `target/` in the canonical checkout, about 1.4 GB, git-ignored | Debug and release builds, test binaries | May be deleted at any time; regenerated by `cargo build` |
| Linux evidence clone | WSL2 Ubuntu, `/home/basho/checkspan-m1`, detached at ac639a4d…, plus its `target/` | Native Linux acceptance evidence for CS-07 | After Basho has reviewed M1 acceptance |
| Linux toolchain | WSL2 Ubuntu, `~/.rustup` and `~/.cargo` (rustup 1.29.1, toolchain 1.98.0 minimal profile with rustfmt and clippy) | Building on native Linux | Retained for later milestone acceptance unless Basho says otherwise |

No git worktree other than the canonical checkout is registered (`git worktree list`). No unpublished commits: `codex/checkspan-m1` and `main` both point at the CS-07 closeout commit after this entry is pushed. Generated fixtures and examples are committed source; scratch generators and evidence scripts used during the session live outside the repository except `run-examples.sh`, which is committed as evidence tooling.

**Acceptance:** M1 accepted with native Windows, native Linux, and hosted evidence on ac639a4d…. **Execution stops here.** CS-08 (M2) requires Basho's explicit approval; nothing in this entry, in elapsed time, or in the earlier full-STS instruction substitutes for that stop, because the milestone boundary is an explicit stop in the approved PSPR.

## CS-08 — Bind immutable snapshots and canonical digests

**Date:** 2026-09-06.  
**Approved scope:** M2 STS (Basho, 2026-09-06).  
**Source SHA before work:** c9ef350e016deb0b06cb45b2cb294cff3854b9fa (CS-07 closeout; `main`).  

**Changed paths:** `src/digests/mod.rs` (new); `src/lib.rs`; `src/contracts/ids.rs` (`Digest::from_sha256`); `Cargo.toml`, `Cargo.lock` (`sha2`, `serde_json_canonicalizer`); `tests/content_binding.rs` (new); `tests/fixtures/canonical/` (two RFC 8785 vectors, one Python cross-check set); this log; the verification ledger; the dependency record.

**Design as implemented:** `artifact_digest(bytes)` is SHA-256 of exact bytes. Envelope digests are RFC 8785 canonical bytes hashed with SHA-256; `envelope_bytes` first rejects any number that is not an integer within ±2^53 (floats and larger integers would be reformatted as doubles), reporting the JSON pointer, and `envelope_digest_from_text` runs the strict parser first so duplicate keys, excess depth, and trailing content reject before anything is hashed. `jcs_bytes` exposes the raw RFC 8785 layer for conformance. Typed digests go through `digest_of(kind, body)`, which hashes `{algorithm: "jcs-sha256@1", kind, body}` so the algorithm version and the envelope kind are part of the bytes: `graph_spec_digest`, `node_spec_digest`, `input_manifest_digest` (evidence ordered by port name and receipts by id, so assembly order does not matter), and `context_digest` over a `VerificationContext` of attempt, full node contract, input-manifest digest, dependency receipts, result digest, and produced evidence. Revision immutability is a store rule for CS-09: a `(graph_id, revision)` is bound to its `graph_spec_digest` and cannot be stored again with a different one.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 92 tests (83 prior + 9 content binding) |
| `cargo deny check` | advisories, bans, licenses, sources ok |
| `cargo audit` | no advisories against 105 lockfile entries |

**Gate evidence (V1/V2):** the RFC 8785 section 3.2.2 example and section 3.2.3 sorting example canonicalize byte-for-byte to the RFC's expected output, and the section 3.2.2 example also matches the hex form printed in the RFC; six independently generated Python vectors (nested structures, escapes, control characters, DEL, BMP Unicode, safe-integer extremes, array root, a graph-run record) match on canonical bytes and SHA-256; SHA-256 known answers for `abc` and the empty input match; a reordered and reformatted document has the same envelope digest as its compact form while six semantic mutations (value, order inside an array, string case, type, removed key, string-vs-number) all change it; graph and node digests survive a serialization round trip, change when a display name or a prompt changes while `GraphRef` identity does not, and the same body under a different envelope kind has a different digest; duplicate keys at the root and nested reject with the key's path before hashing; floats, `-0.0`, `1e30`, 2^53+1, -(2^53+1), and `u64::MAX` reject with their path while 2^53, -2^53, 0, and -1 are accepted, and the raw JCS layer still canonicalizes floats; the input-manifest digest is identical across assembly orders and changes with content or membership; the verification-context digest is deterministic and changes when the attempt, contract, inputs digest, result digest, produced evidence, dependency receipts, or envelope kind change.

**Not claimed:** storage of digests or revision immutability enforcement (CS-09); hosted matrix for this commit (queued on push; recorded at CS-14).

**Acceptance:** CS-08 gate passed locally. Implementation commit SHA is recorded in the CS-09 entry.  
**Open blockers:** none.

## CS-09 — Add transactional run storage

**Date:** 2026-09-06.  
**Source SHA before work:** d36e2ded2180733a8f5896506e0a6c8e854b3f29 (CS-08 implementation commit; `main`).  

**Changed paths:** `src/store/mod.rs` (new); `src/lib.rs`; `Cargo.toml`, `Cargo.lock` (`rusqlite` with bundled SQLite); `tests/store_transactions.rs` (new); this log; the verification ledger; the dependency record.

**Design as implemented:** `Store::open(path)` creates or opens one SQLite file, sets WAL, `synchronous=FULL`, foreign keys, and a 5 s busy timeout, and applies schema version 1 inside a transaction; a file whose `user_version` is newer is refused with `UnsupportedSchema` and left byte-identical. Tables (all STRICT): `graphs` (revision bound to its `graph_spec_digest`), `runs` (foreign key to the graph revision), `events` (append-only log with `AUTOINCREMENT` sequence), `attempts` (one row per started attempt, keyed by run, node, number), `receipts` (foreign key to the sealed attempt), `gate_packets`, `gate_decisions` (one per packet), and `artifacts` (bounded locator, size). Triggers make graphs, attempts, receipts, and events immutable, forbid deleting events, and refuse a `receipt_admitted` event whose receipt row does not exist, so acceptance cannot be written without its receipt. Every write runs inside `Store::transaction` (`BEGIN IMMEDIATE`); composite operations (`create_run`, `seal_attempt`, `admit_receipt`, `open_gate`, `admit_decision`) write the record row and its event in one transaction. Records are stored as their JSON with a 4 MiB bound; reads parse them back through the header-first record parser. `summary` is a derived per-node view (attempt count, receipt count, last verdict) computed from the tables; `row_counts` supports retention accounting. Storing an identical graph revision again is a no-op; different content at the same revision is a `RevisionConflict`.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 104 tests (92 prior + 12 store, all against real SQLite files in a per-test temp directory) |
| `cargo deny check` | advisories, bans, licenses, sources ok |
| `cargo audit` | no advisories against 120 lockfile entries |

**Gate evidence (V1/V3, real SQLite):** opening creates a version-1 WAL file and reopening keeps it; a graph revision stores once, an identical store is a no-op, different content at the same revision is refused with both digests and the original content is what loads, and a later revision stores; a run needs its stored graph, cannot be created twice, and is created together with its `run_created` event; sealing an attempt and admitting a receipt each write the record row and the event in one transaction, duplicates are refused, and the derived summary shows one attempt, one receipt, last verdict `accept`; a receipt for an unsealed attempt and an attempt for an unknown run are refused with nothing else recorded; a raw `receipt_admitted` event without a receipt row is refused by the trigger and leaves no acceptance; a transaction that appends an event, inserts an attempt, a receipt, and an artifact, sees them inside the transaction, and then fails leaves every row count and the event log exactly as before while the earlier sealed attempt is intact, and the same writes commit when the transaction succeeds; direct SQL updates to graphs, attempts, or events and deletes from events are refused by triggers; a gate packet records with `gate_opened`, a decision requires its packet, records with `decision_admitted`, and a second decision for the same packet is refused; an artifact locator over 2048 bytes is refused while one at the bound is stored; a file with `user_version` 7 is refused with `UnsupportedSchema { found: 7, supported: 1 }`, its bytes are unchanged, its own table survives, and no Checkspan table is created; a non-SQLite file is refused unchanged.

**Not claimed:** state transitions (CS-10), dependency resolution (CS-11), claims and competing processes (CS-12), budgets (CS-13), abrupt process termination and native Linux (CS-14); a tamper-proof ledger (a writable local file is not one); the store location relative to a candidate checkout (the caller chooses the path; CS-15/CS-20 decide the default).

**Acceptance:** CS-09 gate passed locally. Implementation commit SHA is recorded in the CS-10 entry.  
**Open blockers:** none.

## CS-10 — Implement the pure state reducer

**Date:** 2026-09-06.  
**Source SHA before work:** 18247ccc7bdc9ac309399eee6c479338592321ba (CS-09 implementation commit; `main`).  

**Changed paths:** `src/state/mod.rs` (new); `src/lib.rs`; `tests/state_transitions.rs` (new); this log; the verification ledger; the dependency record.

**Design as implemented:** `NodeEvent` is the typed vocabulary (`Dispatched`, `ExecutionFinished`, `ReceiptAdmitted`, `GateOpened`, `DecisionAdmitted`, `RetryAllowed`, `Cancelled`) with a fixed store kind and payload for each, and a decoder from stored events that takes the graph's `NodeRef` for the node. `NodeState::apply(&event)` is a pure function from state and event to the next state or a `TransitionError` (illegal from this status, stale attempt, wrong attempt number, wrong packet, unknown or malformed stored event, unknown node); on error the state is unchanged. The legal transitions are: open dispatches the next attempt number exactly or cancels; running finishes (completed keeps it running in a checking phase; failed and timed out are `failed`; cancelled is `cancelled`) or cancels; a checking attempt takes exactly one verdict for its own attempt (`accept` is the only route to `accepted`, `reject` is `rejected`, `undecidable` is `gated` awaiting its packet); rejected and failed nodes reopen only through `RetryAllowed`, escalate through `GateOpened`, or cancel; a gated node takes its packet, then exactly the decision for that packet (`retry`, `approve_action`, `deny_action`, and `record_assessment` reopen it; `cancel` and `revise` cancel it) or cancels; `accepted` and `cancelled` are terminal. `attempts_started` counts every dispatch and never decreases. `RunState` holds one state per graph node, replays a run's stored event log (skipping run-level events, failing on the first illegal one), and projects `NodeView`s in document order; `blocked_reason` is left to the scheduler.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 113 tests (104 prior + 9 state transition) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-09 |

**Gate evidence (V1):** the full transition matrix (9 reachable states including the checking phase and an undecidable node awaiting its packet, times 21 probe events) is asserted cell by cell and the only cell that reaches `accepted` is an admitted `accept` receipt on a checking attempt; an illegal event leaves the state unchanged and names the reason; a wrong attempt number, a stale attempt, and a wrong packet each produce their typed error; a timed-out or crashed attempt is `failed`, never `rejected`, and takes no verdict; every decision kind reopens a gated node without accepting it, keeps the attempt history, and allows the next attempt number, while `cancel` and `revise` cancel it; after a retry the previous attempt's completion and receipt are refused as stale and acceptance names the second attempt's receipt; an undecidable verdict gates the node and its view is inconsistent until the packet opens, then consistent; an eleven-event sequence replays to an identical state and view twice, and swapping any two adjacent events breaks the replay; every reachable state projects a view that passes the CS-04 view rules; a real store's event log (dispatch, sealed attempt, admitted receipt, dispatch and cancel of another node) replays into consistent views for all three graph nodes, removing the dispatch event makes the replay fail rather than accept, and an event for a node outside the graph is refused.

**Not claimed:** who may emit each event (CS-11 dependencies, CS-12 claims, CS-13 budgets); persistence of views (the store event log is the source and replay rebuilds them); hosted matrix for this commit (queued on push; recorded at CS-14).

**Acceptance:** CS-10 gate passed locally. Implementation commit SHA is recorded in the CS-11 entry.  
**Open blockers:** none.

## CS-11 — Resolve run-scoped dependencies and imports

**Date:** 2026-09-06.  
**Source SHA before work:** 4ce7a905f98cc8ff33bd20ffa53852464ef8837f (CS-10 implementation commit; `main`).  

**Changed paths:** `src/deps/mod.rs` (new); `src/lib.rs`; `src/contracts/graph.rs` (`ImportedReceipt`, `GraphRun.admitted_imports`, two identity rules); `src/contracts/ids.rs` (`Timestamp::unix_nanos` / `is_before`); `src/contracts/mod.rs`; `src/store/mod.rs` (`attempt`, `revoke_receipt`, `revoked_receipts`); `src/validation/mod.rs`; `schemas/v1/graph-run.schema.json` (`admitted_imports`); `tests/dependency_binding.rs` (new); `tests/contract_identity.rs`, `tests/store_transactions.rs`, `tests/state_transitions.rs`; fixtures (`contracts/valid/graph-run-with-import.json`, two invalid identity cases); this log; the verification ledger; the dependency record.

**Design as implemented:** `GraphRun` gains `admitted_imports`, a list of `ImportedReceipt` records naming the node of this graph an import stands in for, the receipt in the run that issued it, the result digest and result type it must carry, a subject, an admission time, and an optional expiry; a run cannot list two imports for one node or import a receipt from itself. `DependencyService` resolves a node's declared dependencies against exactly two sources: a node accepted in this run (its view's receipt) or an admitted import for that node; nothing else in the store is consulted, so an older run's accepted node is invisible to a new run unless imported. Every receipt used passes `admissible`: it must be stored, an `accept` verdict, about the exact node revision the dependency names, over a sealed attempt whose result type equals the consumer's `expected_type`, with a result digest equal to the receipt's (and to the import's declared digest), unexpired at `now` (RFC 3339 instants are compared across offsets), and not revoked. `resolve` returns an immutable `DependencySnapshot` of receipt references to pin in the attempt; `recheck` runs the same checks against the pinned snapshot before acceptance, and `recheck_refs` does so for the references stored in an attempt record, refusing missing or extra receipts. `Store::revoke_receipt` appends a run-level `receipt_revoked` event; the receipt row and the historical acceptance are untouched. `blocked_reason` renders the blockers for a view.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed (one scoped `result_large_err` allow with a stated reason in `src/deps/mod.rs`) |
| `cargo test --locked` | passed: 122 tests (113 prior + 7 dependency binding + 1 identity + 1 timestamp unit) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-09 |

**Gate evidence (V1/V2, real store):** a same-run dependency resolves to the exact receipt, run, node revision, and result digest, its snapshot rechecks clean, and a node whose dependency is still open is blocked with a reason naming it; run-0002 of the same graph cannot use run-0001's accepted `cs_patch` (blocked as open) although the receipt is still stored; an explicit import resolves with `Imported` provenance, while an import with the wrong digest, the wrong result type, an unknown receipt, or a past expiry is blocked with the matching reason, and a receipt about `cs_patch@1` cannot be imported for a graph revision where `cs_patch` is revision 2; duplicate imports and self-imports fail the run's identity check and round-trip through the schema; a receipt valid until 2026-10-01 resolves at 2026-09-06 and is blocked as expired at 2026-10-02, the earlier snapshot fails its recheck, and the producing node stays accepted; revoking a receipt appends a run-level event, leaves the receipt row and the historical acceptance untouched, blocks fresh resolution, and fails the recheck of a snapshot pinned before the revocation; a reject receipt, a receipt whose result digest differs from what is expected, and a result type that differs from the consumer's are each refused, and a pinned set with an extra or missing receipt fails the recheck; RFC 3339 instants compare correctly across offsets, fractions, and the epoch.

**Not claimed:** who calls `resolve` at dispatch and `recheck` before acceptance (CS-12 and CS-19); enforcement that a terminal node is never sealed again (the store accepts the row; the scheduler must consult the reducer first, CS-12); hosted matrix for this commit (queued on push; recorded at CS-14).

**Acceptance:** CS-11 gate passed locally. Implementation commit SHA is recorded in the CS-12 entry.  
**Open blockers:** none.

## CS-12 — Implement exclusive scheduling and completion ownership

**Date:** 2026-09-06.  
**Source SHA before work:** d55a50d4c4828963695d4ef2a03dc40579eeaa50 (CS-11 implementation commit; `main`).  

**Changed paths:** `src/scheduler/mod.rs` (new); `src/store/mod.rs` (schema version 2 with `claims`, forward migration, the `Ledger` read trait implemented by the store and by transactions, claim rows, fencing tokens, release); `src/deps/mod.rs` (reads through `Ledger`); `src/lib.rs`; `tests/claim_ownership.rs` (new); `tests/store_transactions.rs` (migration test); this log; the verification ledger; the dependency record.

**Design as implemented:** `Store` schema 2 adds `claims` (run, node, attempt number, owner, fencing token unique per run, the dependency receipts pinned at dispatch, the resources held, active flag, release reason, timestamps); a version-1 file migrates forward inside a transaction and a newer file is still refused. `Ledger` exposes the reads a decision needs (events, receipts, attempts, revocations, claims) on both `Store` and `Tx`, so `claim_next` does everything inside one `BEGIN IMMEDIATE` transaction: replay the run's events, count active claims against `max_active` (default one, the single-worker foundation), resolve dependencies through the CS-11 service, walk the admitted dependency order, skip any ready node whose resource scope conflicts with a resource an active claim holds (exclusive against anything, anything against exclusive), check the reducer accepts the dispatch, then write the claim row with the next fencing token and the `attempt_dispatched` event (owner, fence, pinned receipts) together. The outcome is `Claimed`, `Busy`, or `NothingReady` with every blocked node's reason and every skipped conflict. `complete` seals the attempt and releases the claim in one transaction only if the attempt describes the claimed node and number, carries an execution record and the pinned dependency receipts, the claim row is still active under the caller's exact fence and owner, and the reducer accepts the finish; otherwise nothing is written and the error says whether the claim was released as `completed`, `cancelled`, or `reclaimed`. `cancel` releases the node's active claim and records `node_cancelled`; `reclaim` (by another controller) seals the attempt as `failed` with error code `claim_reclaimed`, naming the owner it took the claim from, and releases the claim as `reclaimed`. `views` fills `blocked_reason` for open nodes. No command is executed.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 131 tests (122 prior + 8 claim ownership + 1 store migration) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-09 |

**Gate evidence (V1/V3, real SQLite):** on the six-node research diamond, claims follow the admitted order (`cs_requirements`, then `cs_implementation`), the default cap makes a second claim `Busy`, fencing tokens are 1 then 2 and never reused, sealed-but-unchecked producers leave synthesis blocked with a reason naming them, and views carry those reasons; **two real controller processes** (the test binary re-spawned twice with distinct identities against one store file) each try to claim the one ready node and exactly one succeeds while the other reports `Busy`, with one claim row, one dispatch event, and the winner's identity on the row; a completion after cancellation is refused as stale with release `cancelled`, writes nothing, and cancelling twice is illegal; a reclaim by a second controller seals the first controller's attempt as `failed` with `claim_reclaimed`, releases the claim as `reclaimed`, and the first controller's later completion is refused with nothing written; a completion with the wrong attempt number, no execution record, a forged fence, a different owner, or different pinned receipts is refused before anything is sealed, and a second completion of the same claim is stale as `completed`; with the cap raised to three, two independent tasks that both need the checkout exclusively serialize (the second is reported as a resource conflict, not busy, and claims after the first completes), two nodes sharing a model endpoint hold two claims at once, and a shared holder blocks an exclusive requester; a scheduler refuses a run of another graph revision and an empty controller identity; a version-1 store migrates to version 2 with its rows intact and an empty claims table.

**Not claimed:** abrupt process termination and reopening (CS-14); retry limits and budgets (`RetryAllowed` is not emitted by the scheduler yet, CS-13); receipt admission and the pre-acceptance recheck call (CS-19); any worker execution (CS-17); hosted matrix for this commit (queued on push; recorded at CS-14).

**Acceptance:** CS-12 gate passed locally. Implementation commit SHA is recorded in the CS-13 entry.  
**Open blockers:** none.

## CS-13 — Enforce retry limits and persistent budgets

**Date:** 2026-09-06.  
**Source SHA before work:** 306989af87ca098203f78b18fea455bfc415d39c (CS-12 implementation commit; `main`).  

**Changed paths:** `src/budget/mod.rs` (new); `src/store/mod.rs` (`Ledger::run`, `Ledger::dispatch_count`); `src/scheduler/mod.rs` (budget gate before any claim, `ClaimOutcome::BudgetExhausted`, the retry's narrowing carried on the claim); `src/lib.rs`; `tests/retry_budget.rs` (new); `tests/claim_ownership.rs`; this log; the verification ledger; the dependency record.

**Design as implemented:** consumption is never kept in memory: `budget_status` counts a run's `attempt_dispatched` events and walks `budget_lineage_ref` through stored run records (refusing loops and unknown runs), so every started attempt counts whatever became of it, and a restart, a re-run with lineage, or a new graph revision run with lineage inherits the spend. The graph declares `total_attempts` and an optional `deadline`; when either is spent the status reports the reason. `disposition` classifies a node's current outcome (`rejected` from a verdict; `timeout` or `infrastructure` from the sealed attempt's execution outcome) and returns `Retry` only if the class is in the node's `allowed_failure_classes`, `attempts_started` is below `max_attempts` (default three including the first), the graph budget has room, and the deadline has not passed; otherwise `Exhausted` with the reason and the policy's route. `apply_retry_policy` runs that decision inside one transaction: a permitted retry records `retry_allowed` with the checked narrowing; an exhausted node routed to `cancel` is cancelled with the reason in the event; one routed to `gate` is left in place for the gate-packet step and reported as such. `check_narrowing` accepts only dropping optional ports and restricting a port's sources to a subset of its approved scope; dropping a required port, naming an unknown port, duplicates, an empty source list, or any source outside the contract (`WidensScope`) is refused, and the acceptance contract is not an input to narrowing at all. The scheduler refuses every claim while the graph budget or deadline is exhausted and carries the pending narrowing on the claim it issues; a human gate decision may reopen a node past its automatic cap, but never past the graph budget.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 138 tests (131 prior + 7 retry and budget) |
| `cargo deny check` / `cargo audit` | not rerun; no dependency change since CS-09 |

**Gate evidence (V1/V3, real SQLite):** a reclaimed attempt, a rejected attempt, and a timed-out attempt each count, the first two are retried with their class named, and the third exhausts the node at its cap of three with route `gate`, writing nothing and leaving the node `failed`; the counts and the decision are identical after closing and reopening the file; a class outside the policy exhausts immediately and the `cancel` route cancels the node with the reason recorded, for a rejection and for a timeout alike; a graph budget of three is spent across run-0001 (two reclaimed attempts) and run-0002 with lineage (one more), after which retry is exhausted with `graph_budget` and the scheduler refuses to claim; a new graph revision run with lineage still sees three consumed and cannot claim, only an explicit larger budget in a further revision leaves room, a lineage naming an unknown run is refused, and all of it reads the same after reopening the file; a passed deadline refuses new claims and retries while an earlier moment allows them; narrowing drops an optional port and restricts a port to a subset while leaving the contract untouched, and refuses a required port, an unknown port, duplicates, dropping and restricting the same port, an empty source list, and a source outside the approved scope; through the store, dropping a required port is refused with nothing recorded, a valid narrowing with a repair hint is recorded and reaches the next claim and its attempt record, and the stored graph digest and contract are byte-identical afterwards; widening a port's sources is refused, and the approved path (a new node revision in a new graph revision) claims normally while the previous run's attempts still count against the lineage budget.

**Not claimed:** the gate packet an exhausted `gate` route needs (CS-21); receipt admission (CS-19); abrupt termination and native Linux (CS-14); hosted matrix for this commit (queued on push; recorded at CS-14).

**Acceptance:** CS-13 gate passed locally. Implementation commit SHA is recorded in the CS-14 entry.  
**Open blockers:** none.

## CS-14 — Accept durable recovery and run inspection

**Date:** 2026-09-06.
**Accepted source SHA:** cb63a14daf5f48476de58dc856283d67e96d9ab1 (`codex/checkspan-m1` and `main`): the CS-13 implementation commit 49c0d6040e5de17852caf3358479a8bae67eff7e plus a test-only exercise and docs.
**Scope:** no product code changed. Two commits: the exercise and guide (cb63a14), then this closeout with the evidence.

**Changed paths:** `tests/durable_recovery.rs` (new, acceptance exercise, clearly labelled); `docs/CHECKSPAN-M2-GUIDE.md` (new recovery instructions); `README.md`; `test-evidence/checkspan/CS-14/{README.md,windows-x64-build.log,windows-inspection.json,windows-retention.txt,linux-x64-build.log,linux-inspection.json,linux-retention.txt}`; `CHECKSPAN-HANDOFF.md` (session handoff); this log; the verification ledger; the dependency record.

**Native Windows x64 (local_native, passed):** on cb63a14, fmt, clippy with warnings as errors, and all 142 tests passed; the durable-recovery suite (a controller killed mid-transaction leaves nothing and a survivor claims with token 1; claims, counters, and fencing survive reopening and a reclaimed owner's completion stays refused; waiting, failed, gated, accepted, and cancelled runs replay identically after reopening with exactly the receipt-backed node accepted) passed and wrote `windows-inspection.json` and `windows-retention.txt`.

**Native Linux x64 (local_native, passed):** WSL2 Ubuntu 26.04 clone fetched to cb63a14, toolchain 1.98.0: fmt, clippy, all 142 tests, and the recovery suite passed; `linux-inspection.json` is byte-identical to the Windows file; retention shows a 102400-byte ledger holding 1 graph, 5 runs, 16 events, 3 attempts, 2 receipts, 1 gate packet, 4 claims.

**Hosted matrix (hosted_ci):** run 34064484446 on cb63a14 (jobs 101570884569 ubuntu-24.04, 101570884404 windows-2025) and `main` run 34064487105 both completed with conclusion `success`, confirmed before this closeout was committed. Every earlier M2 commit also passed the matrix: 34062341691 (d36e2de), 34062626189 (18247cc), 34062922932 (4ce7a90), 34063343474 (d55a50d), 34063899536 (306989a), 34064253801 (49c0d60).

**Gate (V1/V3):** correct checkout and SHA recorded; clean tree; the two-process claim exercise (CS-12) and the abrupt-termination exercise (this prompt) ran on native Windows and native Linux; the store reopened with no fabricated acceptance; counters and stale-completion rejection persisted; replayed views compared equal; bounded retention recorded. Native gates passed on both platforms and the hosted gate passed.

**Storage and worktree closeout (removal requires Basho's authorization; none performed):**

| Item | Location | Purpose | Retirement condition |
| --- | --- | --- | --- |
| Canonical checkout | `C:\Users\17076\Documents\Codex\Work Graph Project`, `codex/checkspan-m1` (= `main`) | Sequential implementation lane | Retained |
| Build output | `target/` in the canonical checkout, about 2.6 GB, git-ignored | Debug and release builds, test binaries | May be deleted at any time; regenerated by `cargo build` |
| Test scratch directories | about 333 directories `checkspan-*` under `C:\Users\17076\AppData\Local\Temp`, each holding a small SQLite ledger or scratch files from one test run | Per-test temporary stores | Removed 2026-09-06 on Basho's instruction after closeout: 329 directories (28.6 MB) under Temp, 41 under WSL `/tmp`, plus the WSL `/tmp/m2-evidence` staging copy; tests recreate what they need |
| Linux evidence clone | WSL2 Ubuntu, `/home/basho/checkspan-m1`, detached at cb63a14, plus its `target/` | Native Linux acceptance evidence for CS-07 and CS-14 | After Basho has reviewed M2 acceptance |
| Linux toolchain | WSL2 Ubuntu, `~/.rustup` and `~/.cargo` (rustup 1.29.1, toolchain 1.98.0) | Building on native Linux | Retained for later milestone acceptance unless Basho says otherwise |

No git worktree other than the canonical checkout is registered. No unpublished commits after this closeout is pushed.

**Acceptance:** M2 accepted at cb63a14 on native Windows, native Linux, and hosted evidence. **Execution stops here.** CS-15 (M3) requires Basho's explicit approval; the milestone boundary is an explicit stop in the approved PSPR.

## CS-15 — Capture exact local patch subjects

**Date:** 2026-09-06.
**Source SHA before work:** 6884547a837e3f568756d8eca8f7ed6c787af66b (CS-14 closeout; `main`).
**Approved scope:** M3 STS ("Run M3 STS now please", 2026-09-06); this prompt is the first of M3.

**Changed paths:** `src/adapters/mod.rs` and `src/adapters/code/mod.rs` (new); `src/contracts/patch.rs` (new record type); `schemas/v1/patch-result.schema.json` (new); `src/contracts/{mod.rs,record.rs,registry.rs}`; `src/validation/mod.rs`; `src/digests/mod.rs` (`patch_subject_digest`); `src/cli.rs` (`inspect` identity for the new record); `src/lib.rs`; `tests/patch_subject.rs` (new); `tests/outcome_contracts.rs`; `tests/fixtures/outcomes/valid/patch-result-{commit,working-tree}.json` and `tests/fixtures/outcomes/invalid/patch-deleted-with-content.json` (new); `README.md`; this log; the verification ledger; the dependency record.

**Design as implemented:** `patch_result@1` names the repository by its sorted root commits (so every clone agrees without a path or remote), the base commit, and the candidate: `commit` for an exact commit, or `working_tree` for a working tree resting on a commit with uncommitted changes. Every path that differs between base and candidate is listed sorted with its change (`added`, `modified`, `deleted`, `untracked`) and, except for deletions, the SHA-256 and size of its exact candidate content; the record also carries the digest of the textual Git patch under pinned diff options, the capture time, and the Git version. The binding identity is the **subject digest**, an envelope digest over repository, base, candidate, and changes, so recapturing an unchanged candidate reproduces it and any content, path, or kind change does not; capture time and tool version are outside it. The adapter drives the installed `git` with explicit argument vectors and `--no-optional-locks`, pins the configuration that could alter output (`core.quotepath`, diff algorithm, prefixes, renames), and reads only: it never stages, stashes, commits, or checks out, and the exercise confirms `HEAD`, the commit count, the stash, the index bytes, and the working-tree diff are untouched by a capture. A clean working tree is captured as its `HEAD` commit; any tracked modification, staged change, or untracked file makes it a `working_tree` candidate whose digest differs from the commit's. Content for a commit candidate is read from the object store, for a working-tree candidate from disk. Ceilings: 4096 changed paths and 64 MiB of content by default, checked before reading (object size through `cat-file -s`, file size through metadata). Refused explicitly: not a repository, unknown revision, a root commit with no base named, unmerged paths, directories (submodules), non-UTF-8 paths, and any path that is not a clean relative `/`-separated path. The contract check additionally requires sorted unique paths, no content on deletions, content on everything else, sizes within 2^53, and the tool named `git`.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 151 tests (142 prior + 8 patch subject + 1 outcome contract) |
| `cargo deny check` / `cargo audit` | not rerun; no crate dependency change since CS-09 |

**Gate evidence (V1/V4, real disposable repositories created with git 2.54.0.windows.1):** a clean tree is captured as its `HEAD` commit with the changed paths and content digests against the named base, naming the commit directly with the base defaulting to its parent yields the identical record and subject digest, recapture is identical, and a capture with no base compares the tree with itself; an unstaged edit, an untracked file alone, and a staged file alone each produce a `working_tree` candidate whose subject digest differs from the clean commit's, while `HEAD`, the commit count, the stash list, the staged diff, and the index bytes are unchanged by the capture; editing the candidate changes the subject and patch digests and restoring the bytes restores the whole record; committing the same bytes is a different subject kind with the same content and patch digests; `dir with spaces/héllo wörld ✓.txt` and `données/résumé – draft.md` are reported exactly as tracked-modified, untracked, and (after committing) added paths on native Windows, and with fixed author, committer, dates, and bytes the commit ids `04f8062d…` and `017df6cb…` and the subject digests `sha256:bacd5262…` (working tree) and `sha256:e3110b8e…` (commit) are pinned as constants for the Linux run to confirm; a deleted path carries no content, an ignored file never appears or moves the digest, and a committed deletion reads the same as a commit candidate; the serialized record validates against the bundled schema, round-trips, and passes `checkspan validate` and `inspect`, while a deletion with content, a repeated or out-of-order path, missing content, and `../escape` are refused by the contract check and by validation; not-a-repository (run in a child process with `GIT_CEILING_DIRECTORIES`, because Git discovery walks upward and the developer's home directory is itself a repository), unknown revision, root commit without a base, two paths against a limit of one, and a four-byte file against a five-byte total, from disk and from the object store alike, are refused with the named error.

**Semantics worth knowing:** untracked files cannot appear in a Git patch without touching the index, so they are identified through `changes` and the subject digest, not the patch digest. The patch digest depends on repository-level attributes (diff drivers, `xfuncname`) and on the Git version, both of which are the repository's own; the subject digest does not. Symlinks are read through; submodules and non-UTF-8 paths are refused. Because discovery walks upward, a wrong path inside a large parent repository is expensive before it fails on the path ceiling; CS-20's CLI should print the discovered root and prefer an explicit repository path.

**Not claimed:** evidence resolution, path containment for evidence, and freshness (CS-16); verifier execution (CS-17/CS-18); receipts (CS-19); the CLI run workflow (CS-20); native Linux confirmation of the pinned constants (queued in the hosted matrix on push and rerun natively at CS-25); hosted matrix for this commit (queued on push).

**Acceptance:** CS-15 gate passed locally. Implementation commit SHA is recorded in the CS-16 entry.
**Open blockers:** none.

## CS-16 — Resolve scoped local evidence

**Date:** 2026-09-06.
**Source SHA before work:** b58abf511ce2516afef4700df5643b4ca3de8193 (CS-15 implementation commit; `main`).

**Changed paths:** `src/evidence/mod.rs` (new); `src/lib.rs`; `tests/evidence_resolution.rs` (new); `tests/fixtures/evidence/graph-with-ports.json` (new); 35 fixture and example files whose `code` ports declared the placeholder `code_ref@1` type, now `patch_result@1` with the bundled schema's digest; this log; the verification ledger; the dependency record.

**Design as implemented:** a run registers named **sources** — a Git repository for `code` ports, a directory with a declared producer, optional attestation document, and optional validity for `logs` ports — and the resolver fills every evidence port of a node from operator **offers** against those sources, or refuses with every reason at once. Enforcement lives here, not in the declaration: an offer whose source name is outside the port's `allowed_source_scope` is refused even if registered, one whose name is in scope but unregistered is refused even if the path exists, and a registered source of the wrong kind, an item of the wrong shape, or a `code` port whose `expected_type` is not exactly `patch_result@1` with the bundled schema digest is refused by name. `code` ports capture the candidate through the CS-15 adapter and freeze its canonical record bytes, subject digest, and locator; `logs` ports read one regular file after full link resolution, refuse any path that is not clean-relative or that resolves outside the canonical source root, and enforce per-file and cross-port byte ceilings checked before reading; `proofs` ports are filled from the pinned dependency snapshot only — an offer for one is refused — with the receipt id, node subject, and accepted result digest, and fail when no pinned receipt or more than one matches the `node:<id>` scope. `rag` and `human` ports cannot be filled by this build: a requirement or an offer is refused as unsupported rather than faked, and an optional one is recorded as omitted. Attestations must exist inside the source and match their declared digest; expected digests, source validity, and evidence `valid_until` are enforced against the resolver's clock. The frozen manifest carries the references, the omitted optional ports, and the CS-08 input-manifest digest over references plus pinned receipts; `verify` re-checks a frozen manifest against the same sources and reports a changed candidate subject, changed file bytes, expired evidence, an unpinned or re-bound proof, or a source no longer registered. A retry narrowing from CS-13 is applied first: an offer for a dropped port is refused and the manifest resolves without it.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 157 tests (151 prior + 6 evidence resolution) |
| `cargo deny check` / `cargo audit` | not rerun; no crate dependency change since CS-09 |

**Gate evidence (V1/V2/V4, real repositories and directories):** code and log ports resolve into a manifest whose references validate, whose digest is reproducible and equals the CS-08 input-manifest digest, and which verifies clean until the candidate or the log bytes change, after which verification names the digest mismatch and the subject change and a fresh resolution has a different digest; proofs come only from the pinned snapshot with the accepted result digest, a snapshot with a different digest or without the pin fails verification, a missing required log blocks with `MissingRequired`, and an offered proof is refused; undeclared, unregistered, wrong-kind, and wrongly-typed sources, wrong-shaped items, duplicate offers, unknown ports, and offers for narrowing-dropped ports are each refused by name while the narrowed manifest resolves without the dropped port; `../`, absolute, and `.`-segment paths, directories, and missing files are refused, a symlink leading outside the source is refused as an escape where symlinks can be created (not on this Windows account, exercised on CI/Linux), and per-file and total ceilings are checked before reading with the file and object-store cases alike; a wrong or missing attestation refuses the whole source, an expected-digest mismatch names both digests, an expired source refuses new work, and a manifest verified after its evidence's validity reports it expired; a required `rag` port, an offered `rag` port, and an offered `human` port are refused as unsupported, never resolved, and an optional unfilled one is recorded as omitted.

**Semantics worth knowing:** locators are readable back by this build (`git:<root>#base=<commit>&candidate=<kind>:<commit>`, `file:<source>/<path>`, `receipt:<id>`); verification of a `working_tree` locator re-captures the live tree, so it honestly reports drift rather than replaying bytes. Containment compares canonicalized paths, so it holds across drive-letter case and 8.3 names on Windows. The placeholder `code_ref@1` type that CS-01's examples introduced is retired; a graph still declaring it is refused at resolution with a `TypeMismatch` naming what this build produces (exercised as the `cs_legacy` node).

**Not claimed:** verifier execution against resolved evidence (CS-17/CS-18); receipts for it (CS-19); CLI exposure (CS-20); retrieval and corpus admission (`rag` stays a refused boundary until CS-R02/CS-R03); signed decisions for `human` ports (CS-21/CS-22); the symlink-escape case on this Windows account (no symlink privilege; the code path is platform-independent and runs where symlinks exist); hosted matrix for this commit (queued on push).

**Acceptance:** CS-16 gate passed locally. Implementation commit SHA is recorded in the CS-17 entry.
**Open blockers:** none.

## CS-17 — Implement the bounded verifier process protocol

**Date:** 2026-09-06.
**Source SHA before work:** 2f419eb798773dafef91fbdf7110ed772c2ef8b5 (CS-16 implementation commit; `main`).

**Changed paths:** `src/verifier_host/mod.rs` (new); `src/lib.rs`; `Cargo.toml` (a `[[test]]` entry marking the verifier suite `harness = false`); `tests/verifier_process.rs` (new); `tests/fixtures/verifier/{request,response-accept,response-reject,response-wrong-protocol,response-unknown-field}.json` (new); this log; the verification ledger; the dependency record.

**Design as implemented:** a verifier is a separate native process under a pinned profile: explicit executable path (optionally pinned by digest and checked before spawn), exact argument vector, working directory, granted environment, wall-clock ceiling, and stdout/stderr byte ceilings. The host writes one versioned `VerifierRequest` (protocol `Exactly<1>`, pinned verifier, exact attempt, subject, claim, required checks, policy, input-manifest digest, evidence references) to the child's standard input from its own thread and requires exactly one `VerifierResponse` (echoed verifier and attempt, verdict, per-check passed/failed/skipped outcomes, bounded reasons) on standard output, with nothing but whitespace after it. Both records are `deny_unknown_fields`, so an unknown field or a foreign protocol version is refused while parsing. Everything else is a **process failure distinct from any verdict**: a mismatched executable digest (never spawned), spawn failure, timeout, cancellation, nonzero exit (even after printing a valid response), output past the ceiling, malformed output, trailing output, and an echo mismatch. The environment is scrubbed with an explicit clear: the child sees only `SystemRoot` (Windows process minimum), scratch variables pointed at the profile's working directory, and the granted pairs. Pipes are drained on their own threads with the ceiling applied while reading, so an over-talkative child is killed rather than deadlocked, and overflow is re-checked at join so a child that exits quickly cannot slip an oversized stream past the poll loop. Timeout and cancellation kill the whole process tree: Windows through `taskkill /T /F` with an explicit argument vector, Unix by spawning the child as its own process group and signalling the group. No shell interprets anything anywhere. This is trusted-local execution; the module says so and claims no sandboxing.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 157 harness tests + 12 verifier process cases (the suite runs without the libtest harness so the same binary can serve as the protocol child with a clean standard output) |
| `cargo deny check` / `cargo audit` | not rerun; no crate dependency change since CS-09 |

**Gate evidence (V1/V4, real native processes; the test binary is the child, its mode set by a granted environment variable):** an accepting and a rejecting verifier round-trip with echoed identity, per-check outcomes, captured bounded standard error, and a duration; a child that prints a *valid accept response* and then exits 3 is a nonzero-exit failure, never a verdict; non-protocol output and a valid response followed by trailing bytes are malformed-output and extra-output failures; 320 KiB of output against a 64 KiB ceiling is an output-too-large failure whether caught live or at exit; a hanging child times out at its 600 ms ceiling, and the grandchild it spawned (writing heartbeats to a file) stops writing after the tree kill — the heartbeat file's size is stable across two later samples; cancellation from another thread kills a hanging child in well under its timeout; the child reports `PATH` and `CARGO_MANIFEST_DIR` absent, the granted variable visible, and scratch pointed at the profile's working directory, the working directory equals the profile's, and a secret sentinel never granted appears in no retained output; a response echoing a different attempt is an echo-mismatch failure; a profile pinning a different executable digest never spawns; the protocol fixtures parse, round-trip, and the wrong-protocol and unknown-field fixtures are refused while parsing.

**Not claimed:** no sandboxing or containment of a hostile verifier (trusted-local scope, recorded in the module and the PSPR); no store writes or receipts (CS-19 maps failures to execution outcomes and admits receipts); no real software checks (CS-18); required-check coverage rules on an accepting response (CS-18/CS-19 own them); hosted matrix for this commit (queued on push).

**Acceptance:** CS-17 gate passed locally. Implementation commit SHA is recorded in the CS-18 entry.
**Open blockers:** none.

## CS-18 — Implement the first software-check verifier

**Date:** 2026-09-06.
**Source SHA before work:** 4531627af1c1ad65c3029162efb4c4ae594b8a1f (CS-17 docs correction; `main`; the CS-17 implementation commit is 8abf4a17ad05c33ef7054debc79b96104d2da816).

**Changed paths:** `src/contracts/software.rs` (new record type); `schemas/v1/software-check-result.schema.json` (new); `src/verifiers/{mod.rs,software.rs}` (new); `src/verifier_host/mod.rs` (reader and kill helpers made crate-visible); `src/evidence/mod.rs` (`code_selection` made public); `src/cli.rs` (`software-verifier` protocol-child subcommand); `src/contracts/{mod.rs,record.rs,registry.rs}`; `src/validation/mod.rs`; `src/lib.rs`; `tests/software_verifier.rs` (new); `tests/outcome_contracts.rs`; new outcome fixtures (`software-result-{accept,reject}.json`, `software-accept-with-failure.json`); 47 fixture and example files whose `software_check_result` type digests now name the real bundled schema; this log; the verification ledger; the dependency record.

**Design as implemented:** the software verifier is a protocol child of this same binary (`checkspan software-verifier --profile <file> --out <file>`), spawned by a controller under a CS-17 host profile; being one executable changes distribution, not the trust boundary. What runs comes from a **pinned validation profile** — a typed document (verifier id and version, check definitions with explicit program path, argument vector, granted environment, and per-check timeout) whose exact bytes must hash to the verifier digest the contract pinned. The profile is an approved document outside the candidate: the candidate cannot choose or alter the programs that run, only make them pass or fail, and weakening the profile changes the pinned identity, so a tampered profile is refused before a single check runs. The candidate comes from the request's one piece of code evidence; the verifier re-captures **the working tree against the frozen base** before and after the checks, whatever the frozen candidate kind, so a commit candidate requires a clean checkout at that commit and any mid-check edit changes the captured subject. A mismatch at the start (stale evidence) or a change across the run ends the process with a subject exit code and no result. Checks run in the checkout with a scrubbed environment plus the profile's grants, bounded output, and a tree-kill on timeout; each records its actual exit code, outcome (`passed`/`failed`/`timed_out`), duration, and stdout/stderr digests. The typed `software_check_result@1` binds attempt, subject, candidate, base, repository roots, profile, environment, required checks, and per-check facts; its contract check refuses `accept` over any non-passed or missing required check and `reject` with nothing failed, and the record is registered, schema-validated, and inspectable like every other record.

**Commands and outcomes (local_native, Windows x64):**

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | passed |
| `cargo test --locked` | passed: 167 harness tests (157 prior + 9 software verifier + 1 outcome contract) + 12 verifier process cases |
| `cargo deny check` / `cargo audit` | not rerun; no crate dependency change since CS-09 |

**Gate evidence (V1/V4, three real process levels: test → `checkspan software-verifier` → check processes, on real repositories):** a passing candidate yields an accepted response and a typed result that validates against the bundled schema, round-trips, carries empty bindings, the exact frozen subject, the pinned profile, the real OS and architecture, and exit 0 with durations for both checks; a candidate that weakens the document its pinned check validates fails that check and is rejected with the failing check named, while the profile-owned check still passes; a profile edited after its digest was pinned exits with the profile code before any check and writes no result; a required check with no pinned definition exits with the contract code; a hanging check is tree-killed at its 500 ms ceiling and recorded as timed out with no exit code and its duration, rejecting the run; a check that edits the candidate and exits cleanly is caught by the post-run recapture, which exits with the subject code and leaves no result; a checkout that drifted after freezing is refused before any check runs; and the golden fixtures cover both conclusions and all three check outcomes while a result concluding accept over a failed check is refused by its contract check.

**Semantics worth knowing:** the verifier needs `PATH` granted in its host profile so it can find `git` and the platform's process utilities; the checks themselves do not inherit it. The verifier's `VerifierRef` digest is the profile document's digest, so profile revisions are new verifier identities by construction. A timed-out check is not a candidate content failure but never a pass; it concludes `reject`, and CS-13's failure classes decide whether that routes to retry.

**Not claimed:** receipt admission for these results (CS-19); CLI run workflow (CS-20); the checks in these tests validate documents rather than build software — the pinned-profile mechanism is what CS-18 establishes, and the M3 pilot's real profile (fmt, clippy, test over a real project) arrives with CS-24's sample project; hosted matrix for this commit (queued on push).

**Acceptance:** CS-18 gate passed locally. Implementation commit SHA is recorded in the CS-19 entry.
**Open blockers:** none.
