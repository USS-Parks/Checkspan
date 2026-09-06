# Checkspan — verification ledger

**State:** M1 in progress on `codex/checkspan-m1`, 2026-09-06. Full STS approved by Basho on 2026-09-06; execution halts at each explicit stop. **No native runtime, retrieval, model/agent, evaluation, hosted CI, operator-signature, or release gate has run.**

The [PSPR](../PLANNING/CHECKSPAN-PSPR.md) defines the required gates and approval scope. Document review is not product acceptance.

## Milestones

| Milestone | Prompts | Authorization | Implementation | Acceptance evidence |
| --- | --- | --- | --- | --- |
| M1 Contract explorer | CS-01–CS-07 | Approved (full STS, 2026-09-06) | CS-01–CS-06 complete; local gates passed | CS-01 SHA 0930294: hosted_ci passed (run 34056847082, ubuntu-24.04 + windows-2025); M1 hosted acceptance recorded at CS-07 |
| M2 Durable run ledger | CS-08–CS-14 | Not approved | Not started | Not run |
| M3 Multi-agent RAG and software pilot | CS-15–CS-24 → CS-R01–CS-R12 → CS-25 | Not approved | Not started | Not run |
| M4 GitHub-backed evidence | CS-26–CS-31 | Not approved | Not started | Not run |
| M5 Packaged pilot | CS-32–CS-36 | Not approved | Not started | Not run |

## Paper-case coverage to implement

| Case | Required behavior | Owning prompts | Evidence now |
| --- | --- | --- | --- |
| PC-01 | Unknown/duplicate ID, bad type, or cycle rejected before dispatch | CS-02–CS-06 | CS-02: duplicate node ID, unknown/foreign/mismatched target, unknown schema version rejected (`tests/contract_identity.rs`, local_native passed). CS-03: bad port/output types rejected. CS-05: oversized, deep, duplicate-key, and unsupported-header documents rejected before interpretation (`tests/document_validation.rs`, local_native passed). CS-06: cycles, unresolved/foreign/mismatched dependencies, and incompatible ports and types rejected at admission (`tests/graph_admission.rs`, local_native passed). |
| PC-02 | Required target remains blocked after upstream failure | CS-06, CS-10–CS-12 | Design only |
| PC-03 | Expired/revoked prerequisite cannot be reused | CS-11, CS-19 | Design only |
| PC-04 | Changed candidate requires matching fresh check evidence | CS-15, CS-18, CS-29 | Design only |
| PC-05 | Correct result shape cannot override failed checks | CS-03, CS-18–CS-19 | CS-03: acceptance pins claim, required checks, verifier, and policy; no record or schema property can carry a status or verdict (`tests/node_contracts.rs`, local_native passed). Behavioural enforcement pending CS-18–CS-19. |
| PC-06 | Worker cannot weaken mandatory checks during retry | CS-13, CS-18 | CS-03: `required_checks` is non-empty and part of the immutable contract; retry-time enforcement pending CS-13/CS-18. |
| PC-07 | Checker crash/unavailable CI produces operational failure | CS-17, CS-20, CS-26 | CS-04: `failed`/`timed_out`/`cancelled` are execution outcomes disjoint from verdicts; none can parse as a verdict (`tests/outcome_contracts.rs`, local_native passed). Live process behaviour pending CS-17/CS-20. |
| PC-08 | Undecidable creates a gate without an acceptance cycle | CS-06, CS-21 | CS-06: a gate joined to the node it resolves by an acceptance chain in either direction is rejected at admission (`tests/graph_admission.rs`, local_native passed). Runtime gate creation pending CS-21. |
| PC-09 | Valid denial cannot be interpreted as approval | CS-04, CS-22 | CS-04: a well-formed `deny_action` decision binds to its packet and `authorizes()` is false; only `approve_action` on the exact action authorizes (`tests/outcome_contracts.rs`, local_native passed). Signature authenticity pending CS-22. |
| PC-10 | Changed/stale action packet invalidates approval reuse | CS-22–CS-23 | CS-04: a decision fails to bind when the packet digest, action target, node, or offered options change (`tests/outcome_contracts.rs`, local_native passed). Expiry and signature checks pending CS-22. |
| PC-11 | Shared resource writes serialize | CS-12, CS-17 | Design only |
| PC-12 | Late completion cannot accept a cancelled/newer attempt | CS-12, CS-19, CS-24 | Design only |
| PC-13 | In-graph proof inputs cannot hide dependencies | CS-06, CS-11 | CS-06: a `proofs` port naming an in-graph node without a declared dependency is rejected; external proof sources are unsupported imports (`tests/graph_admission.rs`, local_native passed). Run-scoped resolution pending CS-11. |
| PC-14 | Smaller retry claim retains the original obligation | CS-03, CS-13, CS-24 | CS-03: the obligation lives in the pinned acceptance contract, not in the attempt; retry policy cannot alter it. Runtime enforcement pending CS-13/CS-24. |
| PC-15 | Partial target closure cannot appear complete | CS-06, CS-23 | CS-06: admission computes the required closure of every target and reports the rest as optional (`tests/graph_admission.rs`, local_native passed). Packet-level closure pending CS-23. |

## RAG-case coverage to implement

| Case | Required behavior | Owning prompts | Evidence now |
| --- | --- | --- | --- |
| RC-01 | Block corpus admission and retrieval. | CS-R01–CS-R02 | Design only |
| RC-02 | Expose no denied content through results, counts, caches, or model context. | CS-R03, CS-R05–CS-R06 | Design only |
| RC-03 | Require fresh snapshot/trace bindings; revoked evidence cannot unblock fresh work. | CS-R02–CS-R03, CS-R10 | Design only |
| RC-04 | Reject the mechanical evidence claim. | CS-R01, CS-R03, CS-R09 | Design only |
| RC-05 | Treat it as data; broker refuses undeclared authority. | CS-R05, CS-R12 | Design only |
| RC-06 | Keep common-origin lineage; no independent-corroboration claim. | CS-R03, CS-R06–CS-R07 | Design only |
| RC-07 | Synthesis and the required packet target stay blocked. | CS-R04, CS-R07, CS-R10 | Design only |
| RC-08 | Preserve reservations/budget history; refuse stale acceptance. | CS-R04, CS-R12 | Design only |
| RC-09 | Expose dispute/insufficiency; required semantic judgment gates. | CS-R08–CS-R11 | Design only |
| RC-10 | No acceptance authority; signed assessment cannot bypass mechanical/software failure. | CS-R09 | Design only |
| RC-11 | Block transmission before dispatch; no sensitive payload logging. | CS-R02, CS-R05, CS-R12 | Design only |
| RC-12 | Do not claim multi-agent RAG acceptance or a quality benefit. | CS-R05, CS-R11–CS-R12, CS-25 | Design only |

V9 requires actual corpus/index and policy enforcement; V10 requires native worker and actual model-call evidence; V11 requires the frozen evaluation set, real comparison runs, and human labels. Deterministic fixtures remain useful but cannot close those live claims. All CS-R01–CS-R12 entries are not approved and not started.

## Evidence record requirements

Use project-local `test-evidence/checkspan/<prompt-id>/` when execution is approved, including CS-R identifiers. Record source SHA/tree, actual commands, platform/tool versions, subject/result/context digests, outcome and failure reason, verifier/profile identity, upstream run IDs, and any operator decision reference.

Record agent/role/attempt identities, actual model/runtime/adapter identity assurance, approved endpoint/egress policy, corpus/index/retriever versions, exact source spans, per-call context/evidence digests, measured calls/tokens/cost and uncertainty, controller overlap, and human-labelled evaluation references for RAG claims.

Use explicit evidence classes: `document_review`, `parser_fixture`, `local_native`, `live_retrieval`, `live_model`, `human_labelled_evaluation`, `hosted_ci`, `signed_operator`, and `packaged_native`. Use explicit outcomes: `not_run`, `blocked`, `failed`, `passed`. A link, mock, or queued run is not a pass.

Retain shareable metadata. Do not commit private signing keys, credentials, private customer content, unredacted logs, or generated build trees. Generated binary artifacts belong in the approved artifact/release channel, with checksums and source bindings in the ledger.

## Draft-only checks

The current review checks document consistency and the completeness of the roster. It does not verify any future command, test, cryptographic integration, process boundary, or runtime behavior.
