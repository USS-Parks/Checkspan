# Checkspan — verification ledger

**State:** M1 accepted at ac639a4 (native Windows, native Linux, hosted); M2 accepted on native Windows and native Linux at cb63a14daf5f48476de58dc856283d67e96d9ab1 and the hosted matrix (run 34064484446) on 2026-09-06; M3 authorized by Basho on 2026-09-06 ("Run M3 STS now please") and in progress: CS-15–CS-20 complete. Full STS approved by Basho on 2026-09-06; execution halts at each explicit stop. **No verifier execution, retrieval, model/agent, evaluation, operator-signature, or release gate has run; M4–M5 are not started.**

The [PSPR](../PLANNING/CHECKSPAN-PSPR.md) defines the required gates and approval scope. Document review is not product acceptance.

## Milestones

| Milestone | Prompts | Authorization | Implementation | Acceptance evidence |
| --- | --- | --- | --- | --- |
| M1 Contract explorer | CS-01–CS-07 | Approved (full STS, 2026-09-06) | Complete at ac639a4 | **Accepted 2026-09-06.** local_native Windows x64 passed; local_native Linux x64 (WSL2 Ubuntu 26.04) passed, 83 tests and identical example digests; hosted_ci passed (run 34059580357, jobs 101557584686 ubuntu-24.04 and 101557584591 windows-2025; `main` run 34059583565). Evidence: `test-evidence/checkspan/CS-07/`. |
| M2 Durable run ledger | CS-08–CS-14 | Approved ("Run M2 STS", 2026-09-06) | Complete at cb63a14 | **Accepted 2026-09-06.** local_native Windows x64 passed; local_native Linux x64 (WSL2 Ubuntu 26.04) passed, 142 tests, identical inspection output; hosted_ci run 34064484446 passed (ubuntu-24.04, windows-2025) and `main` run 34064487105 passed; every earlier M2 commit passed the matrix. Evidence: `test-evidence/checkspan/CS-14/`. |
| M3 Multi-agent RAG and software pilot | CS-15–CS-24 → CS-R01–CS-R12 → CS-25 | Approved ("Run M3 STS now please", 2026-09-06); signer, corpus, model, and evaluation-budget stops remain | CS-15–CS-20 complete; local gates passed | Hosted: per-commit runs; recorded at CS-25 |
| M4 GitHub-backed evidence | CS-26–CS-31 | Not approved | Not started | Not run |
| M5 Packaged pilot | CS-32–CS-36 | Not approved | Not started | Not run |

## Paper-case coverage to implement

| Case | Required behavior | Owning prompts | Evidence now |
| --- | --- | --- | --- |
| PC-01 | Unknown/duplicate ID, bad type, or cycle rejected before dispatch | CS-02–CS-06 | CS-02: duplicate node ID, unknown/foreign/mismatched target, unknown schema version rejected (`tests/contract_identity.rs`, local_native passed). CS-03: bad port/output types rejected. CS-05: oversized, deep, duplicate-key, and unsupported-header documents rejected before interpretation (`tests/document_validation.rs`, local_native passed). CS-06: cycles, unresolved/foreign/mismatched dependencies, and incompatible ports and types rejected at admission (`tests/graph_admission.rs`, local_native passed). |
| PC-02 | Required target remains blocked after upstream failure | CS-06, CS-10–CS-12 | CS-10: a failed or rejected node stays non-accepted until an explicit `RetryAllowed` or gate decision; nothing but an admitted accept receipt reaches `accepted` (`tests/state_transitions.rs`, local_native passed). CS-12: the scheduler never dispatches a node whose dependencies are not accepted and admissible, and reports why. |
| PC-03 | Expired/revoked prerequisite cannot be reused | CS-11, CS-19 | CS-11: expired and revoked receipts block fresh resolution and fail the pre-acceptance recheck while the historical acceptance stands (`tests/dependency_binding.rs`, local_native passed). CS-19: an accepting receipt passes the pre-acceptance recheck inside the admitting transaction; an expired pinned dependency blocks acceptance while the historical acceptance stands (`tests/receipt_admission.rs`, real SQLite, local_native passed). |
| PC-04 | Changed candidate requires matching fresh check evidence | CS-15, CS-18, CS-29 | CS-08: result and context digests bind exact bytes and every context component (`tests/content_binding.rs`, local_native passed). CS-15: the subject digest of a captured candidate changes with any content, path, or kind change and a dirty tree never carries its commit's identity (`tests/patch_subject.rs`, real repositories, local_native passed). CS-18: the verifier re-captures the working tree before and after the checks; stale evidence is refused before any check and a mid-check edit ends the run with no result (`tests/software_verifier.rs`, real processes, local_native passed). CI-side rechecks pending CS-29. |
| PC-05 | Correct result shape cannot override failed checks | CS-03, CS-18–CS-19 | CS-03: acceptance pins claim, required checks, verifier, and policy; no record or schema property can carry a status or verdict (`tests/node_contracts.rs`, local_native passed). CS-18: a `software_check_result` concluding accept over a failed, timed-out, or missing required check is refused by its contract check, and the verifier never emits accept unless every required check ran and passed (`tests/software_verifier.rs`, `tests/outcome_contracts.rs`, local_native passed). CS-19: admission recomputes the context digest over the sealed attempt, contract, manifest, dependencies, and result; forged digests, foreign subjects, wrong verifiers, and stale attempts are refused with nothing written (`tests/receipt_admission.rs`, local_native passed). |
| PC-06 | Worker cannot weaken mandatory checks during retry | CS-13, CS-18 | CS-03: `required_checks` is non-empty and part of the immutable contract. CS-13: a retry can only drop optional ports or restrict sources to a subset; the acceptance contract is not an input to narrowing and the stored contract is byte-identical after a retry (`tests/retry_budget.rs`, local_native passed). CS-18: check definitions live in a pinned profile outside the candidate whose digest is the verifier identity; a tampered profile is refused before any check, and a candidate can only fail the pinned checks, not replace them (`tests/software_verifier.rs`, local_native passed). |
| PC-07 | Checker crash/unavailable CI produces operational failure | CS-17, CS-20, CS-26 | CS-04: `failed`/`timed_out`/`cancelled` are execution outcomes disjoint from verdicts; none can parse as a verdict (`tests/outcome_contracts.rs`, local_native passed). CS-10: the reducer maps timeout and crash to `failed`, never `rejected`, and refuses any verdict afterwards. CS-17: a real child's nonzero exit, timeout, cancellation, oversized, malformed, or trailing output, and a wrong echo are process failures distinct from any verdict, even when the child printed a valid acceptance first (`tests/verifier_process.rs`, real processes, local_native passed). CS-20: a verifier process failure seals the attempt as failed, timed out, or cancelled through the fenced completion and routes through the retry policy; a verification failure leaves the sealed attempt for the next step to retry (`tests/local_run.rs`, real CLI processes, local_native passed). |
| PC-08 | Undecidable creates a gate without an acceptance cycle | CS-06, CS-21 | CS-06: a gate joined to the node it resolves by an acceptance chain in either direction is rejected at admission (`tests/graph_admission.rs`, local_native passed). Runtime gate creation pending CS-21. |
| PC-09 | Valid denial cannot be interpreted as approval | CS-04, CS-22 | CS-04: a well-formed `deny_action` decision binds to its packet and `authorizes()` is false; only `approve_action` on the exact action authorizes (`tests/outcome_contracts.rs`, local_native passed). CS-10: every decision kind reopens or cancels the node and none reaches `accepted`. Signature authenticity pending CS-22. |
| PC-10 | Changed/stale action packet invalidates approval reuse | CS-22–CS-23 | CS-04: a decision fails to bind when the packet digest, action target, node, or offered options change (`tests/outcome_contracts.rs`, local_native passed). Expiry and signature checks pending CS-22. |
| PC-11 | Shared resource writes serialize | CS-12, CS-17 | CS-12: independent nodes needing an exclusive resource an active claim holds are skipped until it is released; shared holders coexist; exclusive against shared is refused (`tests/claim_ownership.rs`, local_native passed). Process-level checkout use pending CS-17. |
| PC-12 | Late completion cannot accept a cancelled/newer attempt | CS-12, CS-19, CS-24 | CS-12: completions are fenced by the claim token; after cancellation or reclaim the old owner's completion is refused with nothing written, and two real processes cannot claim one attempt (`tests/claim_ownership.rs`, local_native passed). CS-19: a late acceptance for a superseded attempt is stale and cannot change the current state; duplicates change nothing (`tests/receipt_admission.rs`, local_native passed). End-to-end repair pending CS-24. |
| PC-13 | In-graph proof inputs cannot hide dependencies | CS-06, CS-11 | CS-06: a `proofs` port naming an in-graph node without a declared dependency is rejected; external proof sources are unsupported imports (`tests/graph_admission.rs`, local_native passed). CS-11: dependencies resolve only within the run or through an explicitly admitted import; another run's green node is invisible otherwise. CS-16: a proof port is filled only from the pinned dependency snapshot with the accepted result digest; offers cannot supply proofs (`tests/evidence_resolution.rs`, local_native passed). |
| PC-14 | Smaller retry claim retains the original obligation | CS-03, CS-13, CS-24 | CS-03: the obligation lives in the pinned acceptance contract, not in the attempt. CS-13: retries carry hints and narrowing only; widening scope is refused and needs a new approved revision, whose run still inherits the budget (`tests/retry_budget.rs`, local_native passed). End-to-end repair pending CS-24. |
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

## M2 acceptance evidence

| Class | Environment | Source SHA | Result | Reference |
| --- | --- | --- | --- | --- |
| local_native | Windows 11 x64, rustc 1.98.0 | cb63a14d… | passed (142 tests; recovery suite with real killed and competing processes) | `test-evidence/checkspan/CS-14/windows-*` |
| local_native | Linux x64 (WSL2 Ubuntu 26.04), rustc 1.98.0 | cb63a14d… | passed (142 tests; inspection identical to Windows) | `test-evidence/checkspan/CS-14/linux-*` |
| hosted_ci | GitHub Actions ubuntu-24.04 + windows-2025 | cb63a14d… | passed | run 34064484446 (jobs 101570884569, 101570884404); `main` run 34064487105 |
| hosted_ci | Every earlier M2 commit | d36e2de, 18247cc, 4ce7a90, d55a50d, 306989a, 49c0d60 | passed | runs 34062341691, 34062626189, 34062922932, 34063343474, 34063899536, 34064253801 |

M2 establishes the durable ledger, reducer, dependency resolution, fenced claims, and budgets as library code exercised by real SQLite files and real processes. V4–V11 have not run; nothing executes a verifier, resolves evidence, calls a model, checks a signature, or packages a release.

## M1 acceptance evidence

| Class | Environment | Source SHA | Result | Reference |
| --- | --- | --- | --- | --- |
| local_native | Windows 11 x64, rustc 1.98.0 | ac639a4d… | passed | `test-evidence/checkspan/CS-07/windows-x64.txt` |
| local_native | Linux x64 (WSL2 Ubuntu 26.04), rustc 1.98.0 | ac639a4d… | passed | `test-evidence/checkspan/CS-07/linux-x64.txt`, `linux-x64-build.log` |
| hosted_ci | GitHub Actions ubuntu-24.04 + windows-2025 | ac639a4d… | passed | run 34059580357 (jobs 101557584686, 101557584591); `main` run 34059583565 |
| hosted_ci | Every earlier M1 commit | 0930294, 918a843, abe323d, 3eecbf0, f7faf41 | passed | runs 34056847082, 34057533885, 34058130081, 34058737149, 34059107159 |

M1 establishes offline validation and admission only. The V3–V11 gates have not run; parser and fixture evidence in M1 does not close any process, storage, credential, retrieval, model, signature, or packaging claim.

## Draft-only checks

Before M1, the review checked document consistency and the completeness of the roster only. Prompts after CS-07 remain unverified until their own gates run.
