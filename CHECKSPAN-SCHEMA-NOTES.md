# Checkspan — schema and workflow notes v0.3

**Paper design, 2026-09-06.** This accompanies the [Checkspan one-pager](CHECKSPAN-ONE-PAGER.md). Checkspan is a brand-new, independent product exploration. These notes carry forward the reviewed contracts and add multi-agent RAG to the proposed first complete pilot; they are not an implemented schema, executable plan, or live-system acceptance. No existing product or API is a prerequisite.

**Execution source of truth:** [Canonical PSPR](PLANNING/CHECKSPAN-PSPR.md), currently a draft for Basho's review. Its proposed stack and staged scope await STS approval.

## What needed correction

| v0 gap | Consequence | Current correction |
| --- | --- | --- |
| `result_type` without an acceptance contract | Well-formed output can be mistaken for correct work. | Pin the claim, checks, verifier, and policy independently of the worker. |
| One mutable node mixes intent, status, and last retry | Prior failures and the original obligation can disappear. | Immutable node revisions, preserved attempts, derived status. |
| Bare node IDs, PR URLs, and evidence locators | A dependency or branch can change underneath an apparent pass. | Bind exact revisions, subjects, evidence digests, and receipts. |
| Shrink the prompt on every rejection | An easier claim can silently replace the requested result. | Narrow repair strategy only; change obligations through explicit revision/decomposition. |
| `undecidable → human_gate` without routing semantics | The gate can wait on the very node it must unblock, or approval can masquerade as proof. | Separate gate context/control references from acceptance dependencies; validate decision payloads. |
| Gate exists only when status is gated | A worker may reach an irreversible action before approval policy is consulted. | Declare authority policy before dispatch; require action-specific authorization at the execution boundary. |
| Dependency independence is the only concurrency rule | Independent nodes can still race on one checkout or resource. | One-worker foundation; bounded read-only research parallelism with resource, ownership, and budget enforcement before pilot acceptance. |
| RAG is only a port name; no agent workflow | Retrieval, generation, provenance, and evaluation never become product behavior. | Explicit corpus admission, separate model-backed roles, typed handoffs, live inference, and CS-R01–CS-R12 gates. |
| A citation or agent consensus is treated as proof | A real source can be quoted inaccurately or fail to support the answer. | Mechanical citation receipts, model assessments, human semantic review, and uncertainty remain separate. |

Public-source check: the attribution should be “Prove2Me, as described by Anthropic,” rather than suggesting that it is an Anthropic product. Anthropic describes a theorem DAG and parallel agent work; the platform describes open/proved states and Lean checking. Checkspan’s schema and retry policy below are our proposal, not claims about Prove2Me internals. [Anthropic](https://www.anthropic.com/research/formalizing-fermats-last-theorem), [Prove2Me](https://prove2.me/about).

## Logical schema, not a wire format

This notation names fields and constraints; it is not runnable YAML or a validated JSON Schema. Required keys, unions, and cross-record invariants must be formalized only if a build is approved.

```text
GraphSpec:
  schema_version, graph_id, revision
  nodes: NodeSpec[]
  agent_profiles: AgentSpec[]  # optional in non-agent graphs; pinned before dispatch
  targets: nonempty NodeRef[]
  budget: total_attempts, deadline, other authorized limits
  supersedes?: GraphRef

GraphRun:
  run_id, graph_ref, admitted_imports, budget_lineage_ref

NodeRef:
  graph_id, node_id, revision

NodeSpec:
  id, revision, kind
  prompt
  deps: {node: NodeRef, output_port, expected_type}[]
  evidence_ports: PortSpec[]
  result_type: {schema_id, version, digest}
  acceptance:
    claim
    required_checks
    verifier: {id, version, digest}
    policy_ref
  retry_policy: {max_attempts, allowed_failure_classes, on_exhaustion}
  authority_policy_ref
  resource_scope
  agent_profile_ref?        # task worker role, never checker authority
  supersedes?: NodeRef

PortSpec:
  name, kind, expected_type, required
  allowed_source_scope, handling_policy_ref

EvidenceRef:
  port_name, locator, subject, version, content_digest
  provenance: {producer, attestation_ref?}
  policy_ref, valid_until?   # required if validity is time-limited

AgentSpec:
  id, revision, role
  worker_adapter_ref, model_profile_ref, prompt_template_digest
  allowed_tools, corpus_scope, egress_policy_ref
  call_context_limits, resource_limits

CorpusSnapshot:
  id, revision, manifest_digest, admission_ref, policy_ref
  documents: {id, version, original_digest, classification, permitted_use}[]
  chunk_manifest_ref, normalization_mapping_ref
  index_digest, retriever_config_ref

CitationSpan:
  corpus_ref, document_id, document_version, document_digest
  chunk_id, original_byte_start, original_byte_end, span_digest
  normalization_mapping_ref?

RetrievalTrace:
  attempt_ref, agent_profile_ref, corpus_snapshot_ref, scope_digest
  queries: {query_ref, query_digest, retriever_config_ref, result_refs}[]
  slices: CitationSpan[]
  errors, usage, sealed_digest

ModelInvocation:
  id, attempt_ref, agent_profile_ref, adapter_digest
  observed_model_identity, identity_assurance, settings_digest
  prompt_template_digest, context_digest, evidence_subset_refs
  request_digest, response_digest, outcome, usage, reservation_ref

RetrievalFindings:
  request_ref, subject_ref, trace_ref
  findings: {claim_id, proposed_text, citation_refs, uncertainty}[]

DraftResearch:
  request_ref, subject_ref, dependency_receipts
  claims: {id, text, category, citation_refs, model_assessment}[]
  missing_evidence, content_digest

ChallengeReport:
  draft_digest, trace_ref
  issues: {claim_id, proposed_disposition, citation_refs, uncertainty}[]

CitedResearch:
  draft_digest, challenge_digest, evidence_receipt_ref
  claim_statuses, operator_assessment_refs, unanswered_questions
  # mechanical checks, model judgment, and human review stay distinct

Attempt:
  run_id, node: NodeRef, number, owner, started_at, finished_at
  repair_hint?              # never edits NodeSpec.acceptance
  agent_profile_ref?, invocation_refs?, retrieval_trace_ref?
  input_manifest: EvidenceRef[]
  dependency_receipts: ReceiptRef[]
  result: {type, artifact_ref, digest}?
  produced_evidence: EvidenceRef[]
  execution: {outcome, error_code?}
  verifier_receipt?: ReceiptRef

VerifierReceipt:
  id, attempt_ref, node: NodeRef
  result_digest, context_digest, subject
  verifier_identity, verifier_version, verifier_digest, policy_ref
  verdict: accept | reject | undecidable
  reason_code?, reason_text?, retry_hint?
  checked_at, validity_conditions
  provenance_ref

GatePacket:
  id, purpose: resolve_work | assess_claims | authorize_action
  context_refs, subject_digest, requested_decision, authority_policy_ref

GateDecision:
  gate_packet_id, gate_packet_digest, authority_identity, decided_at
  decision: retry | revise | cancel | approve_action | deny_action | record_assessment
  assessment?: {claim_matrix_digest, reviewed_claim_ids, outcome, rationale_ref}
  # assessment outcome: adequate_support | inadequate_support | cannot_determine
  scope, expires_at?, authorization_ref?

NodeView:
  run_id, node: NodeRef
  status, current_attempt, blocked_reason?, waiting_on_gate?
```

A graph-scoped `cs_<slug>` may remain the local name, but handoffs resolve the full node reference. References to schemas, policies, and verifier code must themselves be immutable or digest-bound. The proposed implementation stores append-only application records in one transactional local SQLite store owned by Checkspan. A future integration must explicitly map record ownership and reference externally issued receipts without creating competing authority. A vault or identity system is outside this proposal.

Keep node output ports explicit: a task may expose one `result` port initially, with the declared result type. Graph validation must ensure every dependency output exists and matches the consumer’s declared type. A proof port that names an in-graph result must have a matching dependency; external proof receipts require explicit admission under the evidence policy.

No attempt may add an undeclared source or widen source scope. Narrowing optional access must preserve every required input. A proposed expansion needs an explicit authorized contract revision before dispatch.

Contract edits produce a new revision. Attempts and receipts are append-only records; a view may cache the latest status. The producing worker supplies candidates and cannot select a weaker verifier, mutate required checks, or mint acceptance.

## Readiness, acceptance, and historical validity

`open` does not mean ready. Readiness requires all pinned dependency receipts to be accepted and currently admissible, every required input to resolve within its policy, sufficient budget, available resources, and no pending gate. Show the exact blocking node, evidence reference, or resource.

In-graph dependencies resolve within the current graph run. Reusing another run’s output requires an explicit imported receipt binding and the same admission checks; never silently select the latest accepted node by ID.

At dispatch, pin the dependency receipts and resolved input manifest. At verification, seal the candidate and any new evidence generated by the attempt. Validate evidence membership and provenance before invoking the checker. Evidence is data, never an instruction source or a grant of tool authority. Enforcement of source restrictions belongs at the executor/retriever boundary; declarations alone do not enforce them.

A receipt proves its scoped claim about a particular subject at a particular time. It does not follow a moving branch. A newer commit does not erase the old receipt; it requires checks for the new subject. A revoked attestation or expired policy may make an old receipt inadmissible for new work without rewriting history.

Recheck dependency admissibility before recording a downstream acceptance and before using an action authorization. A historical acceptance whose prerequisites are no longer admissible cannot unblock fresh work. Required downstream targets remain incomplete for current use until affected work is revalidated. Pinning and freshness are both needed where the contract requires them.

Receipts need a trusted issuer or trusted local controller binding; a digest alone does not authenticate them. Before acceptance, match the node revision, attempt, result, complete verification context, expected verifier, and policy. Ignore late or duplicate completions that do not match the active attempt. The one-worker foundation and later bounded research workers must survive a crash without accepting an old completion into a newer attempt.

## State transitions

`running` covers execution and checking; an optional phase distinguishes them. A completed attempt is immutable even when the node becomes eligible for another attempt.

| Event | Node view | Required behavior |
| --- | --- | --- |
| Created, or waiting on prerequisites | `open` | Expose a blocking reason until dispatch is permitted. |
| Controller claims ready work | `running` | Allocate the next attempt; freeze inputs and ownership. |
| Authenticated, contract-matching checker pass | `accepted` | Record the receipt and its exact claim. |
| Completed checker rejection | `rejected` | Keep reason and failed-check evidence. |
| Worker/checker crash, timeout, or unavailable service | `failed` | Record an operational error; do not fabricate a verdict. |
| Retry allowed by unchanged contract and remaining budget | `open` | Preserve the previous attempt; increment on next dispatch. |
| Checker undecidable, non-retryable case, or gate policy | `gated` | Create a bounded packet and wait for its designated authority. |
| Approved gate resolution | `open`, new revision, or `cancelled` | Follow the typed decision; never jump from approval to a machine pass. |
| Authenticated semantic assessment | Accepted human decision record; consumer re-evaluates its own contract | Bind reviewed claims and disposition; never bypass a failed mechanical or software check. |
| Cancellation or applicable cap policy | `cancelled` | Stop dependent dispatch; retain evidence and history. |

A result/schema mismatch is a rejection with a precise reason. Transient evidence retrieval failure is operational; an actual evidence-policy violation is rejection/gating under the declared policy. Missing mandatory evidence cannot be silently downgraded to optional evidence.

Count every started attempt, including infrastructure failures. A proposed default is three attempts total per node run, with a finite graph-wide budget and deadline. The approved policy decides whether each failure class is retryable. Any budget extension is explicit; creating a child node or new revision cannot silently evade the existing budget. Gate decisions themselves are authenticated and policy-checked; they are not autonomous worker retries.

## Narrowing and gates without changing the claim

“Repair the null-input branch” can narrow an attempt for a node whose unchanged obligation remains “the patch passes required checks A, B, and C.” “Only prove check A” changes the obligation and cannot be accepted as the original node.

If decomposition is needed, create an explicit graph revision with narrower child contracts and a parent acceptance contract that still enforces the original obligation. Software subtask passes do not compose into whole-system correctness automatically: the parent must name integration checks for the combined artifact.

For an undecidable attempt A:

```text
accepted prerequisites ───> A (gated)
          └──────────────> H (human_gate)
sealed A attempt ──context reference──> H
H decision ──controller routing──> retry / revise / cancel A
```

H must not require A to be accepted. Its context reference points to a sealed attempt, and the controller’s waiting relation is separate from the DAG’s acceptance edges. Validate gate wait chains as well as dependency cycles. No worker runs a graph mutation before that revision has been validated.

An accepted human-gate node means “an authentic, well-formed decision was recorded.” It does not mean “approve.” A denial can be a valid decision. Resumption and action authorization must inspect the decision value, subject, authority, and scope.

An action gate binds the exact action, target, candidate/packet digest, validity, and any one-use condition. A changed subject invalidates reuse. Runtime enforcement still belongs to the execution/trust plane. The v0 software path has no external-action executor.

## Multi-agent RAG in the first complete pilot

```text
request + admitted corpus snapshots + fixed agent profiles
  ├── requirements retrieval task ────┐
  └── implementation retrieval task ──┤
                                     v
                              synthesis task
                                     v
                              challenge task
                                     v
                          research evidence check
                                     v
                         semantic review if required
                                     v
supplied patch ── software check ── review packet
```

These are ordinary typed graph nodes with role bindings, not agents hidden inside a single opaque task. Both retrieval branches may run together; all joins wait for accepted artifacts with explicit limited claims. A retrieval receipt establishes permitted trace/artifact integrity, not the truth of a finding. A synthesis artifact may be accepted as a draft without accepting its propositions.

The controller admits the graph and remains deterministic. Agents can propose queries and findings within their recorded role; they cannot create workers, change contracts, grant access, or select the checker. Two specialists can use the same model with distinct role identities and contexts; that does not make their evidence statistically or epistemically independent.

**Retrieval and frozen inputs.** Freeze CorpusSnapshot, role/source scope, index/retriever configuration, model/adapter profile, and budgets before dispatch. Corpus admission requires attributable origin and permitted use under the declared policy. Only approved UTF-8 text/Markdown enters the first local index; arbitrary crawling and PDFs/OCR are parked. Source changes produce a new snapshot, not a silent refresh.

A query may discover another slice inside that admitted snapshot. Append the query, result, exact original-source span, and per-invocation evidence subset to RetrievalTrace; do not edit the frozen input manifest or add an undeclared source. Downstream tasks consume the sealed accepted trace/findings. Every citation resolves through corpus/document/chunk versions and original byte spans; normalization must retain an exact mapping. Ranking scores and repeated citations are not factual confidence.

Scope filtering must happen before results, counts, caches, or model payloads expose unauthorized material. Recheck source admissibility at emission, downstream acceptance, and packet use. Cache entries include snapshot, role scope, policy, and retriever configuration. Identical bytes from the same origin remain one source. Preserve “no Nation/unattested Indigenous”; unclear classification blocks admission.

**Model and resource boundary.** One approved existing model runtime is accessed through a pinned adapter. Record actual model identity assurance, context/settings and response digests, usage, and any uncertainty in usage after failure. Local inference is preferred; remote transmission needs an explicitly approved endpoint and corpus-egress policy. No model download, new credentials, or paid service is authorized by this paper.

Start the runtime foundation with one worker, then extend it to at most two concurrent read-only research workers. Reserve shared call/token/deadline/cost capacity transactionally before dispatch and reconcile conservatively after a crash. A model retry, tool query, or role change cannot reset the budget. Native processes and actual inference must exercise cancellation and late completion. This is a trusted-local pilot, not a sandbox.

**Grounding and disagreement.** Synthesis distinguishes quotation, paraphrase, inference, and missing evidence. The challenger produces a separate, sealed report and may retrieve counterevidence only within its declared scope. It cannot mutate the draft or negotiate acceptance. A changed draft creates fresh downstream subject bindings; there is no indefinite agent debate.

The deterministic checker validates citation membership and bytes, mandatory coverage fields, subject equality, provenance policy, and disposition of required challenges. These are limited mechanical claims. Semantic support remains a model proposal unless an explicit supported checker or authenticated operator assessment supplies the required evidence. A record_assessment decision binds the exact claim matrix and reviewed claims; accepting its signature authenticates the assessment, not a software pass or universal factual proof.

Missing support or unresolved required semantic decisions produce undecidable and a human gate on sealed context. Model agreement cannot close that gate. If the contract asks for an investigation, the final packet can honestly report unanswered questions; a contract requiring a supported answer remains incomplete until that requirement is met.

**Evaluation.** CS-R11 uses actual inference over a frozen held-out set with source references and human labels, comparing the single-agent and multi-agent workflows under the same declared maximum budget. Record citation integrity, reference-fact coverage, contradiction/abstention, latency, and actual usage. Higher agent count is not evidence of better results; quality failure remains a blocker. The exact gates are in [PSPR Draft 0.2](PLANNING/CHECKSPAN-PSPR.md).

## Software checking within the combined pilot

```text
cs_patch: task
  claim: a typed patch artifact exists against the pinned base
  output: PatchResult@1

cs_ci: check
  deps: exact cs_patch revision / PatchResult@1
  claim: the required trusted checks passed for this candidate
  output: SoftwareCheckResult@1

cs_packet: sink
  deps: exact cs_patch and cs_ci revisions
  claim: a complete packet binds the patch and its required check receipt
  output: ReviewPacket@1

graph.targets: [cs_packet]
```

Here, accepting `cs_patch` establishes artifact integrity only. It cannot satisfy a downstream request for a software-check receipt. The sink requires both the patch and the matching CI result; a green check for another candidate fails the sink contract.

`PatchResult@1` identifies repository, base SHA, candidate SHA or tree/patch digest, and changed artifacts. `SoftwareCheckResult@1` identifies the same candidate, tested SHA/tree, trusted workflow or validation-spec revision, environment, required check IDs, run/attempt IDs, conclusions, and artifact digests. If CI tests a synthetic merge commit, record its base/head relationship and ensure the acceptance policy explicitly permits that tested subject.

The check list and trusted validation definition come from the pinned contract. A candidate cannot satisfy the contract by replacing tests with `exit 0` or weakening the workflow. Changes to required tests or validation policy need separate review/revision. Required checks must have completed with the allowed outcome; pending, missing, skipped, or cancelled checks do not pass by omission.

Local execution and hosted CI produce distinct receipts. Report whichever evidence actually exists. Security-, credential-, policy-, or integration-adjacent claims need the live-system evidence their contract requires; mocks cannot replace that evidence.

If CI rejects a schema-valid patch, retrying `cs_ci` cannot alter the already accepted patch. Repair requires a new patch artifact in a new run (and a new node revision if its contract changes), with fresh downstream bindings. Historical receipts remain attached to the original candidate.

## Paper review cases

These are design walkthroughs and future acceptance cases, not executed tests.

| Case | Required result under v0.3 |
| --- | --- |
| Unknown ID, duplicate ID, dependency/type mismatch, or cycle | Reject graph admission before dispatch. |
| Optional branch fails, but a required target depends on it | Target remains blocked; no success through omission. |
| A required receipt is expired or revoked | Block reuse and dependent acceptance until revalidated. |
| Candidate changes after a green CI run | Require a new subject-matching check receipt. |
| Correctly shaped result violates the named checks | Reject the claim despite schema validity. |
| Worker removes a required check during retry | Reject unauthorized contract change. |
| Checker crashes or CI is unreachable | Operational failure; bounded retry or gate. |
| Checker cannot decide | Gate on sealed attempt context, without an acceptance cycle. |
| Basho denies a gate packet | Record valid denial; do not treat accepted gate status as approval. |
| Basho approves a stale or different action packet | Do not authorize the changed action. |
| Two ready nodes write the same checkout | Serialize; DAG independence is insufficient. |
| A late completion arrives after retry/cancellation | Ignore it for current acceptance; retain audit evidence. |
| A proof input introduces a hidden prerequisite | Reject the missing dependency declaration. |
| A retry narrows to a smaller claim | Revise/decompose explicitly; retain the original obligation. |
| A sink has only some required receipts | Report partial/incomplete; never complete. |

## Additional RAG review cases

These are proposed checks, not executed evidence.

| Case | Scenario | Required result |
| --- | --- | --- |
| RC-01 | Corpus lacks admissible origin/use or has unclear classification | Block corpus admission and retrieval. |
| RC-02 | A role queries outside its permitted sources | Expose no denied content through results, counts, caches, or model context. |
| RC-03 | Corpus bytes, index configuration, or admission changes | Require fresh snapshot/trace bindings; revoked evidence cannot unblock fresh work. |
| RC-04 | A citation uses a fabricated ID, wrong span, or altered quotation | Reject the mechanical evidence claim. |
| RC-05 | Retrieved text instructs an agent to change policy or invoke a tool | Treat it as data; broker refuses undeclared authority. |
| RC-06 | Two agents repeat the same source | Keep common-origin lineage; no independent-corroboration claim. |
| RC-07 | A retrieval branch is absent, failed, or unaccepted | Synthesis and the required packet target stay blocked. |
| RC-08 | Parallel calls hit a cap or return after cancellation/restart | Preserve reservations/budget history; refuse stale acceptance. |
| RC-09 | Sources conflict or cannot answer a required question | Expose dispute/insufficiency; required semantic judgment gates. |
| RC-10 | A challenger or agent vote says the answer passes | No acceptance authority; signed assessment cannot bypass mechanical/software failure. |
| RC-11 | A permitted local source is not permitted at the model endpoint | Block transmission before dispatch; no sensitive payload logging. |
| RC-12 | Only canned agent responses or missing evaluation labels exist | Do not claim multi-agent RAG acceptance or a quality benefit. |

**Review outcome:** the concept is coherent with these corrections. Checkspan’s acceptance, retry, and authority boundaries remain explicit. No runtime, verifier plugin, repository migration, integration, or deployment was performed. The next build decision remains Basho’s; these papers do not authorize execution of a PSPR.

**Revision history.** The supplied v0 remains in the conversation. The [v0.1 schema review](WORK-GRAPH-SCHEMA-NOTES.md) established the workflow corrections. Version 0.2 named the independent product Checkspan and superseded the previous product-comparison positioning. Version 0.3 corrects the omitted multi-agent RAG workflow and aligns with PSPR Draft 0.2; the new scope is proposed, not approved. Existing project exclusions and locks remain in force. The [v0.1 one-pager](WORK-GRAPH-ONE-PAGER.md) is retained as historical context; its proposed product affiliations are not Checkspan prerequisites.
