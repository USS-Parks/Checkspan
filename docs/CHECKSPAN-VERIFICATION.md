# Checkspan — verification ledger

**State:** planning only, 2026-09-06. **No implementation, native runtime, hosted CI, operator-signature, or release gate has run.**

The [PSPR](../PLANNING/CHECKSPAN-PSPR.md) defines the required gates and approval scope. Document review is not product acceptance.

## Milestones

| Milestone | Prompts | Authorization | Implementation | Acceptance evidence |
| --- | --- | --- | --- | --- |
| M1 Contract explorer | CS-01–CS-07 | Not approved | Not started | Not run |
| M2 Durable run ledger | CS-08–CS-14 | Not approved | Not started | Not run |
| M3 Local software pilot | CS-15–CS-25 | Not approved | Not started | Not run |
| M4 GitHub-backed evidence | CS-26–CS-31 | Not approved | Not started | Not run |
| M5 Packaged pilot | CS-32–CS-36 | Not approved | Not started | Not run |

## Paper-case coverage to implement

| Case | Required behavior | Owning prompts | Evidence now |
| --- | --- | --- | --- |
| PC-01 | Unknown/duplicate ID, bad type, or cycle rejected before dispatch | CS-02–CS-06 | Design only |
| PC-02 | Required target remains blocked after upstream failure | CS-06, CS-10–CS-12 | Design only |
| PC-03 | Expired/revoked prerequisite cannot be reused | CS-11, CS-19 | Design only |
| PC-04 | Changed candidate requires matching fresh check evidence | CS-15, CS-18, CS-29 | Design only |
| PC-05 | Correct result shape cannot override failed checks | CS-03, CS-18–CS-19 | Design only |
| PC-06 | Worker cannot weaken mandatory checks during retry | CS-13, CS-18 | Design only |
| PC-07 | Checker crash/unavailable CI produces operational failure | CS-17, CS-20, CS-26 | Design only |
| PC-08 | Undecidable creates a gate without an acceptance cycle | CS-06, CS-21 | Design only |
| PC-09 | Valid denial cannot be interpreted as approval | CS-04, CS-22 | Design only |
| PC-10 | Changed/stale action packet invalidates approval reuse | CS-22–CS-23 | Design only |
| PC-11 | Shared resource writes serialize | CS-12, CS-17 | Design only |
| PC-12 | Late completion cannot accept a cancelled/newer attempt | CS-12, CS-19, CS-24 | Design only |
| PC-13 | In-graph proof inputs cannot hide dependencies | CS-06, CS-11 | Design only |
| PC-14 | Smaller retry claim retains the original obligation | CS-03, CS-13, CS-24 | Design only |
| PC-15 | Partial target closure cannot appear complete | CS-06, CS-23 | Design only |

## Evidence record requirements

Use project-local `test-evidence/checkspan/CS-XX/` when execution is approved. Record source SHA/tree, actual commands, platform/tool versions, subject/result/context digests, outcome and failure reason, verifier/profile identity, upstream run IDs, and any operator decision reference.

Use explicit evidence classes: `document_review`, `parser_fixture`, `local_native`, `hosted_ci`, `signed_operator`, and `packaged_native`. Use explicit outcomes: `not_run`, `blocked`, `failed`, `passed`. A link, mock, or queued run is not a pass.

Retain shareable metadata. Do not commit private signing keys, credentials, private customer content, unredacted logs, or generated build trees. Generated binary artifacts belong in the approved artifact/release channel, with checksums and source bindings in the ledger.

## Draft-only checks

The current review checks document consistency and the completeness of the roster. It does not verify any future command, test, cryptographic integration, process boundary, or runtime behavior.
