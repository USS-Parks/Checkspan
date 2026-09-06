# Checkspan — Canonical Plan / Sequential Prompt Roster (PSPR)

**Initiative:** Checkspan — work you can verify.  
**Version:** Draft 0.2 · 2026-09-06  
**Status:** DRAFT FOR BASHO'S REVIEW. No implementation prompt is approved or started.  
**Canonical repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)  
**Canonical working folder:** `C:\Users\17076\Documents\Codex\Work Graph Project`  
**Canonical plan:** `PLANNING/CHECKSPAN-PSPR.md`  
**Execution log:** [CHECKSPAN-DEVLOG](../docs/CHECKSPAN-DEVLOG.md)  
**Verification ledger:** [CHECKSPAN-VERIFICATION](../docs/CHECKSPAN-VERIFICATION.md)

## 0. Governance

### 0.1 Product and bounded outcome

Checkspan is a new, independent product. It organizes AI-assisted work as a DAG of immutable contracts, candidate artifacts, explicit dependencies, verifier receipts, and human decisions.

The initial user is a developer investigating and reviewing an AI-assisted change. Checkspan must retrieve permitted evidence through distinct agents, produce a cited research brief, check an exact supplied patch, and assemble a packet that exposes uncertainty and pending decisions.

The first complete pilot is **request + approved corpus + supplied patch → multi-agent RAG research + software checks → review packet**. Retrieval-augmented generation (RAG) is part of the proposed M3 pilot, not merely a reserved evidence-port name. Automatic patch generation remains outside this cut.

The roster contains 48 prompts across five independently approvable milestones. Its final target is a packaged, reproducible pilot for native Windows x64 and Linux x64, with real model-backed RAG and software verification. M4 adds GitHub evidence. It does not establish production suitability for untrusted multi-tenant execution.

Draft 0.1 incorrectly narrowed the product to supplied-patch verification and parked multi-agent RAG. This revision corrects that omission for review. The original CS-01–CS-36 identifiers remain stable; CS-R01–CS-R12 execute after CS-24 and before CS-25. No implementation approval is inferred.

### 0.2 Authority and source of truth

1. Basho's current instructions and explicit approval scope.
2. This approved PSPR, including dated addenda.
3. The DEVLOG and verification ledger for actual execution state.
4. [Current product one-pager](../CHECKSPAN-ONE-PAGER.md) and [schema notes](../CHECKSPAN-SCHEMA-NOTES.md).
5. Historical v0.1 papers, retained as context only.

The repository was verified empty when this draft was prepared; GitHub reports `main` as its default branch. Repository wiring and paper publication are administrative work already requested by Basho. They are not completion of CS-01 or authorization to build.

No prior product is a parent, kernel, host, or required integration. Prior project exclusions and locks remain in force. The canonical PSPR example supplied by Basho is a formatting/governance reference only; its product scope, code, and historical worktree rules are not imported.

### 0.3 Approval semantics

**Drafting, publishing, or revising this PSPR does not authorize execution.**

- `Run M1 STS` authorizes CS-01 through CS-07 only.
- Approval of a later milestone requires its earlier dependency milestones to be accepted.
- A request to run the full roster STS authorizes sequential implementation through the listed explicit stops; it does not erase those stops.
- STS authorizes work on the approved prompts, their required local checks, focused commits, and pushes to the approved implementation branch. It does not authorize merges, releases, new credential access, paid services, or deletion.
- The default implementation branch is `codex/checkspan-m1` for M1, continuing an appropriate existing branch for sequential milestones unless Basho specifies otherwise. Branch creation follows the `codex/` prefix.
- Stop after each independently approved milestone. Present its exact evidence, remaining blockers, and next milestone before seeking further scope.
- Do not treat silence, elapsed time, or an agent-generated gate decision as approval.

**Explicit stops:** milestone boundaries; first use of operator signing credentials; private-repository credentials; admitting a new corpus or widening evidence scope; first use of a model endpoint and its data-egress/resource budget; new credential access, paid inference, or model downloads; changed acceptance obligations; merging to `main`; publishing a release; and deletion of branches, worktrees, caches, or local artifacts. A scope already explicitly included in the recorded approval need not be approved again.

Approval to run M4 includes preparing the documented non-sensitive CI fixtures in this repository. Actual fixture branch pushes, draft PR creation where necessary, and live workflow runs must be included in that milestone's recorded authorization before dispatch. No other repository is a test target.

### 0.4 Proposed stack to settle through approval

These are concrete draft defaults, not a claim that Basho has already approved them.

| Area | Proposed default | Override point |
| --- | --- | --- |
| Core and CLI | Rust, one package with library modules and a thin `checkspan` binary | Review before CS-01 |
| Toolchain | Pin a tested stable Rust toolchain and commit `Cargo.lock` | CS-01 records exact versions and supported targets |
| CLI parsing | Reuse `clap` | CS-01 dependency review |
| Data interchange | JSON Schema Draft 2020-12; Rust types via `serde`/`serde_json`; explicit runtime validation | CS-02–CS-05 freeze supported schema features |
| JSON validation | Reuse `jsonschema`; bundle schemas and disable arbitrary remote/file reference fetching | CS-05 proves restricted resolution |
| Canonical digests | SHA-256 for artifact bytes; a maintained RFC 8785 JCS implementation for JSON envelopes | CS-08 selects and validates implementation |
| Durable state | SQLite through `rusqlite`, with bundled SQLite; one authoritative local store | CS-09 validates native builds and durability settings |
| Execution | One controller; one worker through CS-R03; at most two concurrent read-only research workers from CS-R04; exclusive writes and verifier execution | CS-12/CS-17 establish ownership/process behavior; CS-R04 extends the same scheduler |
| Retrieval | Local, versioned UTF-8 text/Markdown corpus; SQLite FTS5 lexical search; immutable source spans | CS-R02/CS-R03 prove corpus admission and actual retrieval; semantic/hybrid retrieval remains an extension point |
| Model execution | One approved existing model runtime through a pinned adapter; local endpoint preferred; role-specific contexts and typed outputs | CS-R05 records exact runtime/model identity, egress permission, context limits, and call/token/cost ceilings before use |
| Agent roles | Two specialist retrieval agents, synthesizer, and challenger, represented as DAG tasks; deterministic checker remains separate | CS-R01 fixes role scopes; CS-R06–CS-R09 implement the handoffs |
| Operator decisions | Import packet-bound OpenSSH SSHSIG decisions from a registered operator public key; signing occurs outside the worker | CS-22 proves signer compatibility and custody boundary |
| GitHub evidence | Optional read-only adapter; public repositories first; existing authenticated tooling only after explicit authorization | CS-26 selects and pins adapter transport |
| Packaging | Native Windows x64 and Linux x64 binaries and checksums | CS-34–CS-35 |
| Interface beyond CLI | Parked | Separate approval |

Keep one package until an actual dependency or publication boundary warrants another. There is no default web service, distributed scheduler, plugin marketplace, database server, or async framework requirement.

SQLite is chosen to reuse transactional crash recovery instead of inventing a journal. Application records are append-only; cached views may be rebuilt. This is not a claim that a writable local database is tamper-proof. [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html), [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/).

### 0.5 Scope, prerequisites, and blockers

**Included:** contract schemas; graph validation; snapshot/digest binding; durable attempts; state transitions; bounded retries; local code/evidence adapters; one software verifier; operator gate packets and signed decisions; admitted text corpora and indexed retrieval; multiple model-backed research agents; bounded read-only concurrency; cited synthesis and challenge reports; mechanical evidence checks and explicit semantic-review status; typed review packets; GitHub check evidence in M4; diagnostics; packaging; native, model, and hosted acceptance.

**Prerequisites to verify in CS-01:** Rust and native build tools; Git; available disk space; executable subprocess support; CI access to this repository; dependency licensing/advisories. Exact installed versions are not assumed.

**Later prerequisites:** a Basho-controlled signing identity for CS-22; an admitted, non-sensitive corpus and an approved real model runtime/budget for CS-R02/CS-R05; an explicitly authorized GitHub fixture lane for CS-31; native Windows and clean Linux pilot environments for CS-35. The model, corpus, and usage allowance have not been selected by this draft. If prerequisites are absent, their live gates remain blocked. Canned model responses cannot substitute for actual inference.

**Parked:** automatic patch generation; training/fine-tuning; embedding/vector infrastructure unless retrieval evaluation justifies a separately reviewed extension; arbitrary web crawling, PDF/OCR and continuous ingestion; untrusted-code sandboxing; distributed/remote agent workers; unbounded agent spawning; multi-user/tenant service; browser UI; Indigenous/Nation corpus integration; mathematical theorem proving; compliance certification; vault/credential custody; automated send/merge/pay/delete; cloud hosting; macOS/ARM packaging; public package-registry publication; commercial pricing. Hosted inference is permitted only within an explicitly approved endpoint, data, credential, and spending scope.

The `rag`, `human`, `logs`, `code`, and `proofs` ports are typed boundaries. `rag` becomes an implemented corpus/retrieval path in CS-R02/CS-R03; all other unimplemented retrieval adapters return unsupported/gated outcomes. Preserve the existing restriction “no Nation/unattested Indigenous”; unclear classification blocks admission. The pilot uses only explicitly admitted non-sensitive material.

### 0.6 Trust and claim boundaries

- A checker proves only its pinned acceptance claim about its exact subject.
- A local controller receipt records what that trusted controller observed. A portable digest proves integrity against an expected digest, not issuer identity by itself.
- Operator signatures authenticate possession of an approved signing key; they do not independently prove physical human presence. Private signing capability must not be available to the worker. If that separation cannot be demonstrated, the operator-authority gate stays unaccepted.
- The local execution pilot permits user-trusted candidates and verifier profiles only. An allowlisted subprocess and a scrubbed environment are not a security sandbox. Arbitrary hostile candidate code requires a separately approved isolation design.
- No product executor can send, merge, access new secrets, pay, or delete. A gate may record or resolve such a request, but v0 cannot perform the external action.
- Evidence and model text cannot select verifiers, alter policy, write accepted status, or issue operator decisions. Retrieval is brokered under the role's frozen corpus/scope; a retrieved instruction cannot add a tool, endpoint, credential, or source.
- Multiple agents or repeated citations are not independent corroboration. Deduplicate common source origins and preserve disagreement.
- Citation existence, exact quotations, permitted provenance, and required-field coverage can be checked mechanically. General claim entailment and relevance are not established by those checks or by an LLM judge. Label semantic assessments as model-proposed, operator-reviewed, disputed, or insufficient; unresolved required semantic judgments route to a human gate.
- Corpus attestation records permitted origin/use under the named policy; it is not a guarantee that the corpus is factually true. Model/runtime identity may be an observed provider declaration rather than an independently verified weight digest.
- There is no global “proved” badge. Reports distinguish schema validity, locally observed checks, authenticated upstream evidence, operator decisions, historical integrity, and current admissibility.

### 0.7 Working discipline

Use the canonical checkout. Before any worktree creation, inventory registered worktrees and disk capacity and record owner, branch, purpose, and retirement condition. Do not create a second checkout for sequential work. Implementation remains one sequential task lane. Product-level research-agent concurrency in CS-R04 does not authorize parallel coding sessions or extra worktrees.

One implementation prompt produces one focused commit after its prescribed pre-commit gate passes. No unrelated cleanup or feature bundles. Preserve user-owned files. Do not suppress required tests or turn a failure into a skipped check.

The milestone acceptance prompts are deliberately separate from implementation: they verify the preceding committed code using hosted/native evidence and may change reports only. A code fix discovered during acceptance reopens the owning implementation prompt; rerun its checks, then repeat acceptance. This avoids claiming hosted proof before a commit exists.

Record prompt ID, changed paths, commands, outcomes, evidence paths/IDs, relevant source SHA, commit SHA, remote SHA, and open blockers. Exact own-commit SHA may be indexed in the following documentation closeout commit; never invent a self-referential hash.

At each milestone, report retained worktrees, dirty state, unpublished commits, generated-data size, and retirement blockers. Removal always requires Basho's explicit authorization.

### 0.8 Multi-agent RAG workflow and contract

The default graph is fixed at admission. An operator supplies the research request, required questions, permitted corpus snapshots, supplied patch, and role profiles. There is no free-running manager that can spawn agents or rewrite the graph.

| Participant | Work and typed output | Authority boundary |
| --- | --- | --- |
| Controller | Validate request; dispatch ready nodes; enforce scope, ownership, budgets, and receipts | Deterministic coordinator, not an agent judge |
| Requirements retriever | Model-guided queries over admitted requirements/docs; emits RetrievalFindings | Read-only retrieval within its assigned scope |
| Implementation retriever | Model-guided queries over admitted code/documentation snapshots; emits RetrievalFindings | Separate context and identity; no checkout writes |
| Synthesizer | Consume both accepted retrieval artifacts; emit DraftResearch with claim-to-source mappings and missing evidence | Cannot enlarge the source set or treat a proposed finding as a proved fact |
| Challenger | Examine the sealed draft and retrieve counterevidence within its own declared scope; emit ChallengeReport | Advisory findings; no acceptance authority |
| Evidence checker | Check exact spans/digests, policy, subject binding, required claims, contradiction disposition, and review status | Issues only its pinned mechanical verdict; semantic uncertainty is explicit |
| Human gate / packet sink | Record scoped operator assessments where required; assemble CitedResearch and matching patch/check receipts | Human assessments retain human provenance; software tests remain mandatory |

Both retrieval branches may run concurrently after corpus admission. Synthesis waits for both; challenge waits for synthesis; final checking binds the sealed draft and challenge evidence. A retry creates a new attempt; changing a sealed draft requires fresh downstream bindings. The workflow has no debate loop or hidden extra retrieval round.

At dispatch, freeze the corpus snapshot, source/role permissions, retriever/index configuration, model profile, and budgets. Query-specific slices discovered within that declared input are appended to the retrieval trace as produced evidence. Record the exact evidence subset and prompt-template/context digest for each model call. Seal the trace and candidate before checking; never mutate the original input manifest. Source expansion requires a separately approved revision.

Add versioned AgentSpec, CorpusSnapshot, RetrievalTrace, CitationSpan, RetrievalFindings, DraftResearch, ChallengeReport, CitedResearch, and ModelInvocation records at CS-R01. Bind agent/node/attempt IDs, model/adapter identity, corpus/document/chunk versions, original-source span mappings, retriever configuration, query/result digests, usage, and uncertainty. Extend signed operator decisions with a packet-bound semantic-assessment payload; recording a valid assessment does not create a machine-check pass.

Shared memory means accepted typed artifacts and the run ledger. Raw cross-agent chat and mutable scratchpads are not shared authority. Model calls, tool requests, subqueries, and retries consume the same persistent run budget; reservations cover concurrent calls and uncertain usage after a crash.

The combined review packet must bind research to the same request and relevant repository/candidate snapshot as the supplied patch. It labels what is citation-checked, model-assessed, operator-reviewed, and software-verified separately. A factual answer can remain incomplete even when its citations are well formed. A complete packet may faithfully report abstention if its declared target is a completed investigation; a target requiring a supported answer remains gated/incomplete.

SQLite FTS5 provides the proposed lexical retrieval primitive; this is an implementation choice, not a claim of retrieval quality or multi-agent superiority. CS-R11 compares actual multi-agent runs with a single-agent baseline before any benefit claim. [SQLite FTS5](https://www.sqlite.org/fts5.html).

## 1. Verification gates — defined before the roster

All tests and commands below are **future requirements**, not results from drafting this document.

| Gate | Required evidence |
| --- | --- |
| V0 — Scope and baseline | Correct checkout, branch, source SHA, clean/owned dirty state, approved prompt, dependency/reuse record, available storage |
| V1 — Local Rust quality | `cargo fmt --all -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; prompt-specific behavioral cases |
| V2 — Contract and graph | Positive/negative schema corpus; duplicate-key rejection; bounded parsing; reference/type checks; cycle rejection; required target coverage; offline schema resolution |
| V3 — Durable runtime | Real SQLite transactions; real competing controller processes; abrupt-process termination/restart; stale completion/cancellation replay; consistent budget accounting |
| V4 — Local software execution | Real native child processes on Windows/Linux; exact candidate binding; trusted validation profile; pass/fail/timeout/malformed-output cases; no secret sentinel in retained diagnostics |
| V5 — Operator authority | Actual packet signed by Basho through the approved external signer; positive import and wrong-key/changed-packet/expired/replayed decision rejection; key separation evidence |
| V6 — Hosted GitHub evidence | Real GitHub run/check/job IDs, actual tested commit, trusted workflow binding, conclusions, and collected artifact digests; missing/pending/skipped/cancelled/mismatched evidence fails closed |
| V7 — Portable packet | Exact required target closure; source/result/receipt bindings; provenance level and freshness explicit; altered or partial packets cannot appear complete |
| V8 — Distribution | Built artifact SHA/checksum, dependency/license/advisory report, clean native install/reproduction, operator instructions, and bounded retained data |
| V9 — Corpus and retrieval | Actual admitted corpus/index; exact source-span reproduction; known-evidence retrieval; changed/revoked/forbidden corpus rejection; scope and cache separation before any model exposure |
| V10 — Multi-agent execution | Real role-specific model calls and native workers; two independent retrieval tasks overlap in controller traces; joins wait; actual cancellation/restart/timeout and persistent shared budgets; endpoint and data-egress enforcement |
| V11 — Research quality and grounding | Held-out questions with recorded reference evidence and rubric; real single-agent/multi-agent comparison; citation integrity, coverage, contradiction/abstention, latency and usage results; model judgments distinguished from operator labels |

V1 applies to code-changing prompts after the crate exists. Docs-only prompts use link/reference/consistency review and the relevant already-produced evidence. Dependency changes also require advisory and license review; no new unresolved critical/high advisory is silently accepted.

Pure reducer tests and protocol fixtures are appropriate for logic. They do not close process isolation, operator authority, credentials, model execution, retrieval-policy enforcement, GitHub integration, or native packaging claims. Those require their stated real-system gates.

Hosted matrix success must be tied to the actual code commit being accepted. “Queued,” “not run,” “local only,” “blocked,” and “passed” are distinct ledger states. A failed or unavailable environment is not a product pass.

## 2. Milestones and approval cuts

| Milestone | Prompts | Usable outcome | Acceptance / stop |
| --- | --- | --- | --- |
| M1 — Contract explorer | CS-01–CS-07 | Offline `validate` and `inspect` CLI; no execution or credentials | V0–V2 + native/hosted contract proof; stop for review |
| M2 — Durable run ledger | CS-08–CS-14 | Persist, inspect, and recover explicit run state; no production worker execution | V1–V3; stop for review |
| M3 — Multi-agent RAG and software pilot | CS-15–CS-24 → CS-R01–CS-R12 → CS-25 | Admitted corpus → distinct research agents → cited brief; supplied patch → actual checks; one review packet | V1–V5 + V7 + V9–V11; stop for review |
| M4 — GitHub-backed evidence | CS-26–CS-31 | Review packet bound to actual GitHub CI and exact candidate | V1, V2, V6, V7; stop for review |
| M5 — Packaged pilot | CS-32–CS-36 | Reproducible native binaries, RAG/model prerequisites, docs, verification record, release candidate | V0–V11 where applicable; explicit release approval |

M1 is the recommended first STS scope. M3 is the first complete product pilot and requires actual multi-agent RAG. CS-15–CS-24 establish reusable software/runtime foundations; finishing them alone does not accept M3. M4 adds hosted evidence without pretending local checks are CI. M5 produces a reviewable release candidate; release publication remains a separate decision.

## 3. Reuse ledger

| Capability | Classification | Source / planned use | Evidence or limitation |
| --- | --- | --- | --- |
| Product/contract decisions | Reuse | Current Checkspan one-pager and schema notes | Repository-local documents, linked above |
| CLI, serialization, JSON validation | Reuse | `clap`, `serde`, `serde_json`, `jsonschema` | Exact versions and licenses selected in CS-01/CS-05 |
| Transactions and recovery | Reuse | SQLite / `rusqlite` | Real failure tests in CS-09/CS-14 |
| Hashing/canonicalization/signature verification | Reuse | Maintained SHA-256/JCS implementation; external OpenSSH SSHSIG | No custom cryptographic format or key vault |
| Existing product source | Extraction | None planned | No predecessor codebase has been inspected or authorized for extraction |
| Paper contracts | Extension | Turn the reviewed record sketches into versioned schemas | CS-02–CS-04; no hidden claim weakening |
| Software/GitHub adapters | Implementation at an established seam | The verifier/evidence protocols created in this roster | CS-17, CS-18, CS-26–CS-29 |
| Corpus index | Reuse | SQLite FTS5 in the existing SQLite dependency | CS-R02/CS-R03 verify build support, limits, ranking, and source-span binding; no separate vector server |
| Model runtime | Implementation at an established seam | Approved existing inference runtime and pinned adapter through the worker/process boundary | CS-R05; no custom model server or orchestration framework selected |
| Multi-agent scheduling and artifact handoff | Extension | Existing claims, budgets, evidence broker, gate records, receipts, and packet sink | CS-R04/CS-R10; no second scheduler, memory authority, or work ledger |
| Research role/grounding contracts | Genuinely new work | Role profiles, corpus admission, citation mapping, challenge and assessment rules | CS-R01–CS-R12; no borrowed framework assumed to enforce policy |
| Graph admission, reducer, receipt policy, gate routing, packet closure | Genuinely new work | Checkspan-owned modules | Behavior-driven gates below |
| CI and test tooling | Reuse | Cargo/GitHub Actions and established Rust test utilities | Pin third-party actions; limit permissions; no invented CI framework |

Library availability is not proof of suitability. Recheck maintenance, licenses, advisories, supported targets, and needed semantics when the relevant prompt starts. Current primary references: [clap](https://docs.rs/clap/latest/clap/), [Serde](https://serde.rs/), [jsonschema](https://docs.rs/jsonschema/latest/jsonschema/), [JSON Schema 2020-12](https://json-schema.org/draft/2020-12), [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785).

## 4. Ordered prompt roster

All entries start **NOT STARTED / NOT APPROVED**. Each prompt inherits the governance and relevant common gates. Paths below are proposed destinations, not claims that implementation files exist.

### Phase A / M1 — Contract explorer

#### CS-01 — Establish the reproducible Rust baseline

- **Depends on:** explicit M1 STS approval.
- **Objective:** create the smallest buildable Checkspan package.
- **Work:** one Cargo package; pinned toolchain/lockfile; thin CLI help/version; module layout; dependency decision record; ignore generated state; Windows/Linux CI definition with minimal permissions. Reuse the existing papers and repository wiring.
- **Gate:** V0/V1 pass locally; help/version smoke tests pass; workflow definition is reviewed and its configured commands run locally. Record native prerequisites and lockfile decisions. Hosted behavior is not claimed until CS-07.
- **Deliverables:** `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/`, CI definition, dependency record.

#### CS-02 — Define identities and graph envelopes

- **Depends on:** CS-01.
- **Objective:** make graph, node, revision, and run identity unambiguous.
- **Work:** versioned GraphSpec/GraphRun/NodeRef schemas; required targets; exact-reference rules; schema registry IDs; unknown-version behavior; graph budget lineage. Keep display names separate from identity.
- **Gate:** V1/V2 identity fixtures distinguish same local slug across graphs/runs/revisions; unknown schema versions and missing target declarations reject.
- **Deliverables:** `schemas/v1/`, `src/contracts/`, `tests/contract_identity.rs`.

#### CS-03 — Define immutable node and evidence contracts

- **Depends on:** CS-02.
- **Objective:** encode what a node must deliver and what it may consume.
- **Work:** NodeSpec, AcceptanceSpec, PortSpec, typed dependency outputs, retry policy, authority policy, resource scope, evidence/provenance/freshness fields. Define each node kind and conditional required fields.
- **Gate:** valid/invalid fixtures prove mandatory checks and inputs cannot disappear; wrong port/output types and unsupported policy versions reject. A valid shape never implies a behavior pass.
- **Deliverables:** node/evidence schemas, Rust contract types, `tests/node_contracts.rs`.

#### CS-04 — Define attempts, receipts, and gate decision records

- **Depends on:** CS-03.
- **Objective:** keep execution outcomes, verifier verdicts, and operator decisions distinct.
- **Work:** Attempt, VerifierReceipt, GatePacket, GateDecision, and derived NodeView contracts; explicit result/context/subject bindings; failure vs rejection; typed decision payloads and provenance levels.
- **Gate:** records cannot express acceptance without its required receipt bindings; an authentic denial remains a denial; a timeout cannot parse as a verifier verdict. Golden fixtures cover every variant.
- **Deliverables:** run/receipt/gate schemas and `tests/outcome_contracts.rs`.

#### CS-05 — Implement bounded offline document validation

- **Depends on:** CS-04.
- **Objective:** admit only well-formed, supported documents without ambient retrieval.
- **Work:** JSON parser limits, duplicate-key detection, strict schema validation, Rust deserialization, bundled schema resolver, field-path diagnostics, unsupported-feature rejection. Disable unapproved network and filesystem `$ref` resolution.
- **Gate:** V1/V2; oversized/deep/duplicate-key inputs and traversal/remote references reject before execution or network access. Chosen Draft 2020-12 keywords have positive and negative coverage.
- **Deliverables:** `src/validation/`, schema corpus, `tests/document_validation.rs`.

#### CS-06 — Implement graph admission and read-only commands

- **Depends on:** CS-05.
- **Objective:** turn individually valid records into a valid, inspectable DAG.
- **Work:** duplicate/unresolved IDs, cycles, output type compatibility, hidden proof dependencies, gate wait-chain validation, target closure, unsupported external imports. Add `checkspan validate` and `inspect` with stable JSON output/exit codes and a small valid/invalid example set.
- **Gate:** V1/V2; chains, diamonds, isolated targets, unresolved references, cycles, incompatible ports, and gate cycles produce deterministic outcomes. Inspect performs no dispatch and creates no run store.
- **Deliverables:** `src/graph/`, CLI commands, `examples/`, `tests/graph_admission.rs`.

#### CS-07 — Accept the contract-explorer milestone

- **Depends on:** CS-06.
- **Objective:** establish a usable, documented offline milestone.
- **Work:** run the committed CS-06 implementation on native Windows and Linux; obtain the hosted matrix result; reproduce valid/invalid examples through CLI-only instructions. Change reports/docs only.
- **Gate:** V0–V2; exact code SHA and hosted run IDs recorded; examples give expected status/exit codes; no worker or credential path exists. Any code defect reopens its implementation prompt.
- **Deliverables:** M1 evidence and beginner instructions. **Stop for Basho's M2 approval.**

### Phase B / M2 — Durable run ledger

#### CS-08 — Bind immutable snapshots and canonical digests

- **Depends on:** accepted M1 and M2 approval.
- **Objective:** give contracts, input manifests, and results stable content bindings.
- **Work:** raw artifact SHA-256; JCS envelope canonicalization; exact input/context digest construction; versioned digest algorithms; immutable revision creation. Reject unsupported numbers/encodings before hashing.
- **Gate:** V1/V2; official/interoperable canonicalization vectors match; property reordering preserves envelope digest; semantic mutation changes it; duplicate keys and nonconforming numeric input reject.
- **Deliverables:** `src/digests/`, canonical fixtures, `tests/content_binding.rs`.

#### CS-09 — Add transactional run storage

- **Depends on:** CS-08.
- **Objective:** persist attempts and receipts without partial state updates.
- **Work:** SQLite schema/migrations; graph/run/attempt/receipt relations; foreign keys; transaction boundaries; append-only application records; derived views; bounded artifact references. Store outside the candidate checkout.
- **Gate:** V1/V3 against real SQLite; interrupted writes cannot leave acceptance without its receipt; rollback preserves the prior state; unknown/newer DB schema fails without destructive repair.
- **Deliverables:** `src/store/`, migrations, `tests/store_transactions.rs`.

#### CS-10 — Implement the pure state reducer

- **Depends on:** CS-09.
- **Objective:** centralize legal state transitions.
- **Work:** reducer for open/running/accepted/rejected/failed/gated/cancelled; typed events; explicit terminal/current-attempt behavior; read-only derived views. Acceptance requires an admitted receipt event.
- **Gate:** V1; transition matrix tests reject illegal jumps, direct worker acceptance, timeout-as-rejection, and approval-as-pass. Replaying the same ordered events yields the same view.
- **Deliverables:** `src/state/`, `tests/state_transitions.rs`.

#### CS-11 — Resolve run-scoped dependencies and imports

- **Depends on:** CS-10.
- **Objective:** prevent a node from consuming the wrong accepted output.
- **Work:** same-run dependency resolution; explicit imported receipt admission; revision/result-type/subject matching; freshness and revocation checks; immutable dependency snapshot at dispatch and recheck before acceptance.
- **Gate:** V1/V2; an older run's green node cannot silently satisfy a new run; changed/expired/revoked dependencies block fresh work while preserving historical receipts.
- **Deliverables:** dependency admission service and `tests/dependency_binding.rs`.

#### CS-12 — Implement exclusive scheduling and completion ownership

- **Depends on:** CS-11.
- **Objective:** let one controller claim ready work and accept only its current attempt's completion.
- **Work:** transactional claims, single active worker, ownership/fencing tokens, deterministic ready ordering, resource-conflict refusal, duplicate/late event handling. No production command execution yet.
- **Gate:** V1/V3; two real controller processes cannot claim one attempt; cancellation/reclaim invalidates old completions; independent nodes sharing an exclusive resource serialize.
- **Deliverables:** `src/scheduler/`, `tests/claim_ownership.rs`.

#### CS-13 — Enforce retry limits and persistent budgets

- **Depends on:** CS-12.
- **Objective:** make retries finite without changing the original obligation.
- **Work:** per-node attempt cap (default three including first), graph-wide attempt/deadline budget, retryable error classes, repair hints, optional-input narrowing, budget lineage across revisions/subtasks, exhaust-to-gate/cancel policy.
- **Gate:** V1/V3; every started attempt counts, including failure; restart and revision cannot reset budget; narrower hints preserve mandatory checks; widened source scope requires a new approved contract.
- **Deliverables:** retry/budget policy and `tests/retry_budget.rs`.

#### CS-14 — Accept durable recovery and run inspection

- **Depends on:** CS-13.
- **Objective:** prove the committed runtime survives process loss and explains its state.
- **Work:** native two-process claim and abrupt-termination exercises; reopen the store; inspect waiting, failed, gated, and cancelled runs; compare replayed views; record bounded data retention. Use controlled protocol fixtures, clearly labelled.
- **Gate:** V1/V3 on the committed code in native Windows/Linux; no accepted state is fabricated after restart; counters and stale-completion rejection persist; hosted matrix passes.
- **Deliverables:** M2 evidence and recovery instructions. **Stop for M3 approval.**

### Phase C / M3 — Multi-agent RAG and software pilot

#### CS-15 — Capture exact local patch subjects

- **Depends on:** accepted M2 and M3 approval.
- **Objective:** represent the candidate that will actually be checked.
- **Work:** Git repository identity; base/candidate commits; dirty-tree and patch digest semantics; changed-path list; immutable candidate snapshot metadata; explicit no-implicit-commit behavior.
- **Gate:** V1/V4 using a real disposable fixture repository; dirty files cannot masquerade as a clean commit; candidate mutation changes the subject; paths with spaces and Unicode work on native Windows.
- **Deliverables:** `src/adapters/code/`, PatchResult schema, `tests/patch_subject.rs`.

#### CS-16 — Resolve scoped local evidence

- **Depends on:** CS-15.
- **Objective:** freeze the actual allowed inputs before a verifier runs.
- **Work:** code/log/proof reference resolution; canonical path containment and symlink handling; provenance metadata; mandatory-port enforcement; source/size/freshness limits; explicit unsupported RAG retrieval refusal until CS-R02/CS-R03 implement the admitted adapter.
- **Gate:** V1/V2/V4; undeclared sources, escaped paths, missing required evidence, invalid attestations, and mismatched digests fail before dispatch; no forbidden corpus data is used.
- **Deliverables:** `src/evidence/`, `tests/evidence_resolution.rs`.

#### CS-17 — Implement the bounded verifier process protocol

- **Depends on:** CS-16.
- **Objective:** run a registered verifier and distinguish its output from process failure.
- **Work:** versioned JSON request/response; pinned executable/profile; explicit argv/cwd; minimal environment; bounded stdout/stderr; timeout/cancellation and process-tree cleanup behavior; no shell interpolation. The first verifier may be a subcommand of the same binary.
- **Gate:** V1/V4 with real native processes: success, nonzero exit, malformed JSON, extra output, timeout, cancellation, and child descendants. Secret sentinels are absent from outputs/store. Record trusted-local scope; do not claim sandboxing.
- **Deliverables:** `src/verifier_host/`, protocol fixtures, `tests/verifier_process.rs`.

#### CS-18 — Implement the first software-check verifier

- **Depends on:** CS-17.
- **Objective:** produce a typed result for the required checks on one exact candidate.
- **Work:** pinned validation profile and tool versions; required check IDs; actual per-check exit/outcome/duration; candidate recheck; report/artifact digests. Validation definitions come from an approved source outside candidate-controlled modifications.
- **Gate:** V1/V4; known passing/failing candidates yield correct results; skipped/missing checks do not pass; replacing tests/workflow with `exit 0` cannot meet the unchanged profile; source changes during checking invalidate the binding.
- **Deliverables:** software verifier, SoftwareCheckResult schema, `tests/software_verifier.rs`.

#### CS-19 — Admit and issue verifier receipts

- **Depends on:** CS-18.
- **Objective:** record acceptance only for a matching, trusted checker result.
- **Work:** validate protocol version, verifier identity/digest, policy, attempt ownership, subject, all context hashes, dependency admissibility, and verdict; transactional receipt/state update; explicit provenance level.
- **Gate:** V1/V3/V4; wrong-verifier, wrong-subject, substituted-evidence, forged-acceptance, stale-attempt, and duplicate messages cannot change current acceptance. Receipt integrity and issuer trust remain separately reported.
- **Deliverables:** `src/receipts/`, `tests/receipt_admission.rs`.

#### CS-20 — Connect the local run workflow

- **Depends on:** CS-19.
- **Objective:** operate patch → check through the CLI.
- **Work:** candidate submission, validated run creation, dispatch, persistent progress, inspect/resume/cancel commands, stable machine output. Candidate generation stays external; the prompt remains intent.
- **Gate:** V1/V3/V4; a real submitted patch reaches an accepted software-check result; failing/missing evidence leaves the correct visible state; restart does not duplicate successful work or dispatch blocked dependents.
- **Deliverables:** CLI run commands and `tests/local_run.rs`.

#### CS-21 — Create bounded human gate packets

- **Depends on:** CS-20.
- **Objective:** stop undecidable/exhausted work with a concrete decision packet.
- **Work:** sealed attempt context, original obligation, rejection evidence, proposed bounded options, requested authority, exact subject/digest, expiry; separate wait/control references from accepted dependencies.
- **Gate:** V1/V2/V3; undecidable and exhausted runs produce complete packets; the gate never waits for the paused node's acceptance; cyclic waits reject; a missing operator decision leaves the run gated.
- **Deliverables:** gate packet command and `tests/gate_packets.rs`.

#### CS-22 — Verify externally signed operator decisions

- **Depends on:** CS-21 and explicit signer-use authorization.
- **Objective:** admit only decisions bound to the reviewed packet and configured operator identity.
- **Work:** OpenSSH SSHSIG verification using a dedicated namespace and pinned allowed-signers policy; signed canonical decision payload; key/identity/scope/expiry checks; one-use replay control; retry/revise/cancel routing. Signing/private-key handling stays outside Checkspan workers.
- **Gate:** V1/V3/V5: Basho signs an actual packet; valid bounded retry is admitted; wrong signer, changed packet, stale subject, expired/replayed decision, and valid denial cannot authorize an action or manufacture a checker pass. Demonstrate that worker context has no signing capability.
- **Deliverables:** gate decision importer, signer runbook, real operator evidence. Missing live signer evidence blocks this prompt.

#### CS-23 — Build complete typed review packets

- **Depends on:** CS-22.
- **Objective:** assemble a reviewable result without strengthening its claims.
- **Work:** sink target closure; patch/check subject equality; JSON packet plus readable summary; receipt/artifact index; partial/incomplete labels; offline integrity verification with explicit provenance/freshness limits.
- **Gate:** V1/V7; missing targets, another candidate's green run, altered digests, inadmissible receipts, and stale decisions cannot produce a complete packet. A local attestation is never labelled an independently authenticated certificate.
- **Deliverables:** ReviewPacket schema, export/verify commands, `tests/review_packets.rs`.

#### CS-24 — Exercise rejection, repair, and resume end to end

- **Depends on:** CS-23.
- **Objective:** make failure and recovery work through the actual local pipeline.
- **Work:** real-process acceptance scenarios for reject → narrower retry, infrastructure failure, gate → signed resolution, cancellation, late completion, and changed-candidate repair. Reuse existing components rather than add another orchestration path.
- **Gate:** V1/V3/V4/V7; retrying a CI/check node cannot mutate an accepted patch; repair creates fresh subject bindings and retains budget/history. All scenario packets state their actual assurance.
- **Deliverables:** `tests/local_acceptance.rs`, reusable benign sample project and scenario instructions.

### Phase C continuation / M3 — Multi-agent RAG

These prompts extend the runtime already established by CS-15–CS-24. The CS-R prefix preserves the original roster IDs. Execute the following sequence before CS-25; none is optional for the revised first complete pilot.

#### CS-R01 — Define agent, retrieval, and research contracts

- **Depends on:** CS-24 and M3 approval covering the revised RAG scope.
- **Objective:** make multi-agent RAG an explicit typed DAG.
- **Work:** AgentSpec and per-node role/profile references; CorpusSnapshot, RetrievalTrace, CitationSpan, RetrievalFindings, DraftResearch, ChallengeReport, CitedResearch, ModelInvocation; signed semantic-assessment payloads; fixed two-retriever → synthesis → challenge → checker graph. Specify dispatch manifests versus appended retrieval traces and separate artifact-integrity, citation, semantic, and software claims.
- **Gate:** V1/V2; missing role/source permissions, hidden handoffs, wrong-run findings, malformed spans, and agent-authored acceptance reject. Valid examples expose uncertainty and operator-assessment provenance. No new node kind or second ledger is required.
- **Deliverables:** RAG schemas, role profiles, graph examples, contract tests.

#### CS-R02 — Admit versioned local corpora

- **Depends on:** CS-R01 and recorded approval for the exact pilot corpus.
- **Objective:** index only permitted, attributable source material.
- **Work:** bounded UTF-8 text/Markdown import; corpus/document versions and hashes; original byte spans and normalization mapping; issuer/admission records; source classification and allowed use/egress; immutable snapshot manifests and FTS5 index identity. Reuse existing SQLite and external signing/admission boundaries.
- **Gate:** V1/V2/V9 using a real non-sensitive corpus and an actual approved admission record; missing/invalid attestation, unclear classification, path escape, oversized input, altered bytes, and revoked sources cannot be admitted or reused. No Nation/unattested Indigenous material is retrieved.
- **Deliverables:** corpus import/inspect commands, benign corpus fixture, versioned manifests, native admission evidence.

#### CS-R03 — Implement scoped retrieval and citation provenance

- **Depends on:** CS-R02.
- **Objective:** return reproducible slices from the frozen permitted corpus.
- **Work:** brokered bounded queries; FTS5 ranking with deterministic tie-breaking; role/source filters before ranking and result exposure; top-k/byte limits; source-origin deduplication; complete query/result trace; exact quote/span references; cache keys including snapshot, role scope, policy, and retriever configuration. Search results remain evidence, never instructions.
- **Gate:** V1/V9 against the actual index; known source spans reproduce; empty results are explicit; forbidden canary text cannot appear in results, counts, model payloads, cache hits, or diagnostics; changed snapshot/policy invalidates reuse. Verify FTS5 support on both targets.
- **Deliverables:** retrieval broker, citation resolver, retrieval/provenance tests and native evidence.

#### CS-R04 — Extend scheduling to bounded research concurrency

- **Depends on:** CS-R03.
- **Objective:** run independent retrieval tasks concurrently without racing authority or budgets.
- **Work:** extend the existing scheduler to at most two read-only research workers; exclusive write/verifier resources; role ownership and completion fencing; transactional reservations for calls/tokens/deadline/cost where applicable; crash-safe reconciliation of uncertain usage. No dynamic agent spawning or distributed workers.
- **Gate:** V1/V3/V10 with actual native worker processes; retrieval tasks overlap, synthesis waits for both receipts, conflicting access serializes, and cancellation/restart cannot double-spend a reservation or accept a stale completion. This proves scheduling, not real inference until CS-R05.
- **Deliverables:** scheduler extension, shared-budget tests, native process traces.

#### CS-R05 — Connect one approved model runtime

- **Depends on:** CS-R04 and explicit runtime, corpus-egress, credential, and usage authorization.
- **Objective:** execute bounded model calls through a pinned adapter.
- **Work:** select one existing runtime; record model/version as actually observable, adapter digest, role prompt template, sampling/context settings, endpoint policy, timeout/cancellation behavior, and usage ceilings. Reuse the process boundary; accept only typed model output and brokered retrieval requests. No new model download, provider account, or secret store by default.
- **Gate:** V1/V4/V10 with actual inference; malformed output, unavailable runtime, context overflow, disallowed endpoint/tool, and exhausted budget fail visibly. Outgoing payloads contain only admitted context; credentials and operator signing capability are unavailable to model-controlled actions. Missing live runtime evidence blocks this prompt.
- **Deliverables:** pinned model adapter/profile, usage records, runtime runbook, actual invocation evidence.

#### CS-R06 — Implement the two specialist retrieval agents

- **Depends on:** CS-R05.
- **Objective:** collect complementary findings through distinct model-backed workers.
- **Work:** requirements and implementation roles with separate IDs, contexts, prompts, and source scopes; bounded model-proposed search queries through the broker; typed findings with exact citations and uncertainty. Accepted artifacts are the only cross-agent handoff; common source origins remain common.
- **Gate:** V1/V9/V10 using real model calls; each agent retrieves its required known evidence; neither can read the other's undeclared context or widen its sources/tools; shared-source findings are not counted as independent corroboration. Record all queries/calls, including unsuccessful attempts.
- **Deliverables:** role implementations/profiles, retrieval findings, actual two-agent run evidence.

#### CS-R07 — Produce cited synthesis with explicit gaps

- **Depends on:** CS-R06.
- **Objective:** assemble the retrieval findings into a reviewable draft.
- **Work:** synthesize only after both retrieval artifacts are accepted for their limited artifact/provenance claims; claim-to-source matrix; required question coverage; separate quotations, paraphrases, inferences, and missing evidence; exact request/repository/candidate binding. Draft acceptance establishes a well-formed artifact, not factual correctness.
- **Gate:** V1/V7/V10 with actual synthesis; missing retrieval input blocks dispatch; invented source IDs and subject mismatches reject; duplicate citations do not inflate coverage; unanswerable questions remain visibly unanswerable.
- **Deliverables:** synthesizer role, DraftResearch artifacts, synthesis evidence.

#### CS-R08 — Add a scoped challenge agent

- **Depends on:** CS-R07.
- **Objective:** surface conflicting evidence and unsupported claims before final checking.
- **Work:** a separate challenger context receives the sealed draft and its declared evidence; it can retrieve counterevidence within its approved corpus scope; produces per-claim ChallengeReport entries and source references. It cannot modify the draft, negotiate with the checker, or grant acceptance.
- **Gate:** V1/V9/V10 on real model calls over seeded contradictory and insufficient-evidence cases; challenge output retains disagreement and source lineage; a challenged draft requires explicit disposition. False negatives remain measured quality failures, never proof that no contradiction exists.
- **Deliverables:** challenger role/profile, challenge reports, live challenge cases.

#### CS-R09 — Enforce evidence checks and semantic review status

- **Depends on:** CS-R08.
- **Objective:** accept only the exact evidence claim the checker can establish.
- **Work:** deterministic checks for source membership, digests/spans, quotation fidelity, mandatory coverage fields, subject equality, allowed provenance, and disposition of required challenges; explicit citation-checked/model-assessed/operator-reviewed/disputed/insufficient states. Required unresolved semantic judgments produce undecidable and a packet-bound human gate; signed assessments remain operator evidence.
- **Gate:** V1/V2/V5/V9; fabricated citations, altered quotes, omitted conflicts, invalid assessments, and wrong-source answers cannot pass their required claim. Actual signed semantic assessment is validated without bypassing a failed mechanical check or any software test. Agent consensus cannot set accepted status.
- **Deliverables:** research evidence checker, assessment importer extension, real gate and adversarial citation evidence.

#### CS-R10 — Integrate research into the review packet

- **Depends on:** CS-R09.
- **Objective:** deliver the combined research and software workflow through the existing CLI.
- **Work:** extend run/inspect/export/verify and the packet sink; bind CitedResearch to the same request and relevant supplied-patch subject; include agent/model provenance, corpus snapshots, citations, challenges, assessments, and usage. Retries reuse approved scope, preserve budgets, and invalidate downstream bindings when a draft or corpus changes.
- **Gate:** V1/V3/V7/V9/V10; a real combined run produces the required packet; a missing agent branch, stale corpus, changed candidate, failed software check, or unresolved required assessment prevents completion. An investigation that abstains is labelled accordingly and cannot satisfy a target requiring a supported answer.
- **Deliverables:** combined CLI example, packet extensions, actual end-to-end research/software evidence.

#### CS-R11 — Evaluate retrieval quality and multi-agent value

- **Depends on:** CS-R10 and authorized evaluation usage budget.
- **Objective:** measure whether the extra agents produce a useful research result.
- **Work:** freeze a small held-out set and rubric before collecting outputs: at least 12 questions, including answerable, contradictory, and unanswerable cases; retain reference source spans outside agent context. Compare single-agent and multi-agent runs on the same corpus, model profile, union of permitted source/tool scopes, and maximum run budget. Report retrieval recall/coverage, operator-labelled claim support, contradiction handling, abstention, latency, actual calls/tokens, and cost when known.
- **Gate:** V9–V11 with actual inference and recorded human labels; every accepted citation resolves; all designated insufficient/conflicting cases are deferred or explicitly qualified; at least 80% of required reference facts for answerable cases are correctly reported and cited. Record failed runs and resource tradeoffs. Missing data or a failed quality floor blocks this gate; no superiority claim follows from agent count or an LLM-only judge.
- **Deliverables:** versioned evaluation set/rubric, baseline and multi-agent results, scorecard and limitations. Further tuning must preserve the held-out record or introduce a fresh set.

#### CS-R12 — Exercise live RAG failure and authority boundaries

- **Depends on:** CS-R11.
- **Objective:** prove the actual pipeline fails closed on the declared boundary cases.
- **Work:** exercise prompt-injection fixtures, source revocation, retrieval outage, model timeout, malformed citation, cancellation during parallel calls, restart with uncertain usage, denied source/endpoint, and late completion through the real corpus/model/worker path. Use only benign canaries and approved runtime/data.
- **Gate:** V3/V4/V5/V9/V10; no unauthorized source/tool/policy change is admitted, no fabricated acceptance occurs, dependency joins and budgets remain correct, and required undecidable work stays gated. Report exactly what the live cases establish; no sandbox or broad security-audit claim.
- **Deliverables:** real RAG boundary evidence, failure/recovery instructions, updated verification mapping.

#### CS-25 — Accept the multi-agent RAG and software pilot

- **Depends on:** CS-R12.
- **Objective:** prove a reviewer can reproduce the first complete product pilot.
- **Work:** run the committed CLI on native Windows and Linux against the admitted corpus and benign patch; use actual model-backed retrieval, synthesis, challenge, evidence checks, software checks, and operator decisions. Follow CLI-only instructions; inspect pass/reject/abstain/gate/retry paths. Reports/docs only.
- **Gate:** V1–V5/V7/V9–V11; record exact code SHA, corpus/index/model/adapter identities, real agent calls and controller overlap, per-claim citations, usage, native outputs, hosted matrix, real operator decisions, and retained data sizes. Canned responses, software checks alone, or parser-only evidence cannot accept M3.
- **Deliverables:** combined M3 evidence, scorecard, and operator guide. **Stop for M4 approval.**

### Phase D / M4 — GitHub-backed evidence

#### CS-26 — Establish the read-only GitHub evidence boundary

- **Depends on:** accepted M3 and explicitly scoped M4 approval.
- **Objective:** access only the repository/run evidence the user authorized.
- **Work:** public API transport first; repository/host allowlist; fixed endpoint families; pagination, rate limits, timeouts; optional existing credential helper only with explicit authorization; no raw token persistence or write API.
- **Gate:** V1/V2; real read-only access to Checkspan repository metadata succeeds; wrong hosts/repos and write operations are refused; missing auth/rate limiting yields a visible operational failure. Sensitive credential pathways cannot be closed with mocks.
- **Deliverables:** GitHub adapter profile and `tests/github_boundary.rs`.

#### CS-27 — Normalize check and workflow evidence

- **Depends on:** CS-26.
- **Objective:** represent what GitHub actually reports without losing identity.
- **Work:** workflow/run/attempt/job/check IDs, producer/app identity, workflow source revision, tested SHA, status/conclusion, timestamps, artifact references/digests, and pagination. Preserve upstream facts separately from Checkspan policy.
- **Gate:** V1/V2; fixtures cover complete pagination, reruns, duplicate names, missing records, pending/skipped/cancelled results, rate-limit errors, and malformed responses. These parser results are labelled structural until CS-31.
- **Deliverables:** GitHub evidence schemas and `tests/github_evidence.rs`.

#### CS-28 — Apply trusted CI acceptance policy

- **Depends on:** CS-27.
- **Objective:** decide whether the named required checks passed under the approved workflow.
- **Work:** pin required producer/workflow/check identities; require terminal allowed outcomes; select the policy-defined current attempt; reject candidate-controlled policy changes and unrelated green results.
- **Gate:** V1/V2; name spoofing, a prior successful run followed by failure, omitted checks, changed workflow definitions, and unapproved producer identity cannot pass.
- **Deliverables:** CI policy evaluator and `tests/github_policy.rs`.

#### CS-29 — Bind PR/head/base and tested merge subjects

- **Depends on:** CS-28.
- **Objective:** make the tested artifact's relationship to the submitted candidate explicit.
- **Work:** head/base/tree identity; synthetic merge commit relation where the policy permits it; changed-base/head handling; freshness recheck; no “green PR” shortcut.
- **Gate:** V1/V2 with real Git objects and recorded API fixtures; moved head/base, unrelated merge commit, or omitted subject relation rejects. A permitted merged-subject result is labelled as such.
- **Deliverables:** subject binding and `tests/github_subject.rs`.

#### CS-30 — Prepare the authorized live CI fixture lane

- **Depends on:** CS-29 and recorded permission for this repository's fixture operations.
- **Objective:** create a controlled way to collect real positive and negative GitHub evidence.
- **Work:** benign pass/fail and policy-change candidates, push-triggered CI, optional draft PR scenario for synthetic-merge evidence, expected result matrix, bounded fixture names and retention plan. No secrets or privileged `pull_request_target` execution.
- **Gate:** V0/V1; fixture code/check profiles behave locally; workflow permissions and trigger/subject mapping are reviewed; generated operations are limited to this repo. This setup gate does not claim live integration success.
- **Deliverables:** fixture definitions and live acceptance procedure; commit/push only within recorded M4 scope.

#### CS-31 — Accept actual GitHub evidence end to end

- **Depends on:** CS-30.
- **Objective:** prove Checkspan consumes actual hosted checks correctly.
- **Work:** run the authorized fixture lane; ingest real GitHub results; build a packet; repeat after failed/rerun/changed-subject cases; record raw shareable IDs and digests. Reports/docs only.
- **Gate:** V6/V7; current pass accepted, deliberate failure rejected, pending/skipped/missing check blocked, moved candidate invalidated, and allowed synthetic-merge relation confirmed if supported. Record tested source SHA, workflow definition, run IDs, and final packet digest. A private-auth claim requires its own live authorized evidence.
- **Deliverables:** M4 evidence and hosted adapter guide. **Stop for M5 approval.**

### Phase E / M5 — Packaged pilot

#### CS-32 — Bound diagnostics and operational resource use

- **Depends on:** accepted M4 and M5 approval.
- **Objective:** keep failure inspection useful and resource consumption predictable.
- **Work:** metadata-first diagnostics, explicit log/artifact limits, parser/graph limits, output truncation markers, retention accounting, cancellation status, and storage-size reporting. No raw prompt/source/secret collection by default.
- **Gate:** V1/V3/V4; oversized input/output and noisy failures stop within configured limits; a secret sentinel does not appear in diagnostics; full disks and unavailable artifacts produce explicit failures without damaged acceptance history.
- **Deliverables:** diagnostic export and `tests/operational_limits.rs`.

#### CS-33 — Close the schema/workflow regression matrix

- **Depends on:** CS-32.
- **Objective:** demonstrate the intended invariants survive combinations of failures.
- **Work:** property-based and adversarial cases tied to the 15 original workflow cases and 12 RAG cases; independent review of the current diff/contracts; fix findings within the relevant original prompt boundaries.
- **Gate:** V1–V7/V9–V11 as applicable; every case maps to actual evidence or a documented out-of-scope claim; no unresolved defect permits incorrect acceptance, scope widening, replay, or gate bypass. No assertion of a broad security audit from a test matrix alone.
- **Deliverables:** regression mapping, review findings/dispositions, exact tested SHA.

#### CS-34 — Prepare reproducible pilot packages

- **Depends on:** CS-33.
- **Objective:** produce distributable Windows/Linux CLI candidates.
- **Work:** release-mode builds, archive layout, checksums, dependency/license/advisory inventory, supported verifier/model-runtime prerequisites, corpus/index formats, version/release-note draft, and build provenance. No registry publication or release tag.
- **Gate:** V1/V8; each package contains its intended binary and documentation; checksums verify; core validate/inspect work without a development checkout; declared external verifier tools remain explicit.
- **Deliverables:** local/CI candidate packages and draft release notes tied to source SHA.

#### CS-35 — Reproduce installation and recovery from candidate packages

- **Depends on:** CS-34.
- **Objective:** prove the packaged workflow on clean supported environments.
- **Work:** native Windows and clean Linux installation, contract validation, admitted-corpus import, actual multi-agent research, local sample verification, restart recovery, signed assessment/decision import, GitHub packet example, and uninstall/retention instructions. Reports/docs only; no automatic deletion.
- **Gate:** V4–V11 using the actual candidate artifacts and approved real inference; record binary checksums, code SHA, OS/tool prerequisites, operator actions, results, and any limitations. A headless Linux run does not substitute for native Windows evidence.
- **Deliverables:** M5 pilot acceptance record and clean-install guide.

#### CS-36 — Present the release decision and close the milestone

- **Depends on:** CS-35.
- **Objective:** give Basho a concrete, evidence-backed release candidate to approve or defer.
- **Work:** reconcile the roster/DEVLOG/verification ledger; list complete and parked scope; review artifact provenance, release notes, distribution terms, remote SHA, and worktree/storage inventory. Identify the exact proposed release source and checksums, including any documentation-only closeout commits.
- **Gate:** all included milestone gates pass with no fabricated evidence; candidate artifacts and limitations are reviewable; no required work remains hidden. Stop for explicit release authorization. If granted, perform only the approved tag/release publication and verify published checksums/URLs; otherwise record “candidate accepted, publication pending.”
- **Deliverables:** release decision packet and milestone closeout. Publication is not a prerequisite for accepting the private/local pilot.

## 5. Settled invariants and configurable defaults

The following invariants are not retry knobs: unchanged acceptance obligation; required evidence; no speculative dependent execution; exact subject/receipt binding; no approval-to-machine-pass transition; immutable attempt history; real evidence for live claims; and human ownership of privileged actions.

Draft defaults for review: one controller; one worker until CS-R04, then at most two concurrent read-only research workers with exclusive writes/verifiers; four role-specific agent identities using one approved model profile unless revised; FTS5 text retrieval; three total attempts per node run; finite graph-wide attempt/deadline and model-call/token/cost budgets; public GitHub evidence first; all required targets for completion; offline schema resolution; trusted-local execution only; no external-action executor.

Concrete size/time ceilings are measured and pinned in the relevant implementation prompt. Increasing them within a previously approved resource envelope may be a configuration change; widening evidence scope, changing acceptance, adding an adapter, or granting authority requires explicit revision/approval.

When an accepted artifact changes, create a new candidate/run binding. When the acceptance contract changes, create a new node revision. Preserve budget lineage and historical receipts in both cases.

## 6. Completion and evidence rules

A prompt is complete only when its own gate passes, its focused changes are committed, and the DEVLOG identifies evidence and commit. A milestone additionally requires its acceptance prompt, relevant hosted/native results, a verified remote SHA, and storage/worktree closeout.

The verification ledger must distinguish local/native, hosted, real retrieval, actual model calls, signed operator, human-labelled evaluation, parser/fixture, and not-run results. Links to GitHub actions must include real run IDs and source SHAs. Keep shareable metadata and digests in the repository; raw secrets, signing keys, unredacted logs, and private customer artifacts do not belong there.

The complete planned pilot requires accepted M1–M5, reproducible supported-platform packages, actual multi-agent RAG meeting its recorded quality floor, correct pass/reject/abstain/gate/retry behavior, exact research/candidate/CI subject binding, usable instructions, and no unacknowledged critical acceptance defect. Deferred features remain explicitly outside that claim.

No signing credential, commercial license, release tag, domain registration, or paid service is chosen by this draft. Distribution terms must be settled before public binary publication.

## 7. Review decisions and first approval

This draft asks Basho to review:

1. Rust + one local SQLite-backed CLI as the initial implementation.
2. Multi-agent RAG plus supplied-candidate software verification as the first complete pilot; automatic patch generation stays parked.
3. M1 as the first STS authorization boundary.
4. The trusted-local execution limitation and externally signed operator decision design.
5. GitHub integration as separately approved M4, and packaged pilot/release review as M5.
6. The fixed agent roles, two-reader concurrency cap, lexical retrieval default, corpus admission, real-model budget gate, and stated research-quality floor. Exact corpus/model selection remains pending before those prompts execute.

**Suggested approval after review:** “Run Checkspan M1 STS.”  
Until that authorization arrives, all 48 prompts (CS-01–CS-36 and CS-R01–CS-R12) remain unstarted and unapproved.

## 8. Revision history

- **Draft 0.2, 2026-09-06:** corrects the omitted multi-agent RAG workflow after Basho's review comment. Adds CS-R01–CS-R12 inside M3, V9–V11, role/corpus/model contracts, real-agent evidence and evaluation, and updates downstream pilot/package acceptance. Preserves all original CS IDs. The first-pilot placement is a draft recommendation pending Basho's review; no execution or model/corpus access is authorized.

- **Draft 0.1, 2026-09-06:** first granular Checkspan PSPR, prepared at Basho's request after naming the independent product and selecting its repository. No implementation authorization is inferred.
- The current product papers replace earlier product-affiliation framing. Historical drafts remain traceable; no legacy project is reopened.

Primary technical references checked while drafting: [Rust exhaustive matching](https://doc.rust-lang.org/book/ch06-02-match.html), [OpenSSH ssh-keygen](https://man.openbsd.org/ssh-keygen), [GitHub check runs](https://docs.github.com/en/rest/checks/runs). The Prove2Me reference in the product paper remains a public-pattern citation, not an implementation dependency.
