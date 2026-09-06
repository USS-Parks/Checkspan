# Checkspan — product one-pager v0.3

**Standalone product exploration · paper only · no build authorization**

*Work you can verify.*  
**Canonical repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)  
**Working folder:** `C:\Users\17076\Documents\Codex\Work Graph Project`  
**Execution plan:** [Canonical PSPR — draft for review](PLANNING/CHECKSPAN-PSPR.md). Implementation requires explicit STS approval.  
**Revision:** 2026-09-06; adds multi-agent RAG to the proposed first complete pilot and preserves Checkspan's independent product boundary. See [schema notes and review cases](CHECKSPAN-SCHEMA-NOTES.md).

**Product.** Checkspan organizes multi-agent retrieval, cited research, software checks, and explicit human decisions into connected work. The name combines the check at each acceptance boundary with the span of work across dependencies.

**First customer problem.** A developer investigating and reviewing an AI-assisted change needs attributable research, visible disagreement, and exact patch/check evidence. Checkspan runs specialist retrieval, synthesis, and challenge agents over an approved corpus, then assembles their cited brief and the supplied patch's check receipts into one review packet.

**Thesis.** Work becomes a DAG of typed claims and artifacts. An immutable node contract declares intent, inputs, and acceptance criteria. A worker produces a candidate; a designated checker records a verdict against that exact candidate and evidence. Acceptance establishes only the stated contract. Authorization to act is separate.

**Public precedent.** Anthropic describes using Prove2Me, developed by Tianyi Peng and collaborators at Columbia, to coordinate theorem dependencies and parallel agents. Prove2Me documents open/proved states and Lean checking. These public patterns motivate the design; they do not make software tests equivalent to mathematical proof. [Anthropic account](https://www.anthropic.com/research/formalizing-fermats-last-theorem), [Prove2Me explanation](https://prove2.me/about).

**Schema: keep the contract separate from execution history.**

| Record | Contents |
| --- | --- |
| Graph | `schema_version`, `graph_id`, `revision`, `nodes[]`, required `targets[]`, total budget. Validate resolved references, unique IDs, acyclicity, compatible types, and target coverage. |
| Node contract | Namespaced `id` + `revision`; `kind: task \| check \| human_gate \| sink`; intent `prompt`; typed `deps[]`; `evidence_ports[]`; versioned `result_type`; pinned `acceptance`; retry and authority policies; optional pinned agent-role profile. Immutable during a run. |
| Attempt | Run/node/attempt and agent IDs, owner, timestamps, frozen input manifest, exact dependency receipts, model/profile/context and usage records where applicable, candidate digest, produced retrieval trace/evidence, execution outcome, verifier receipt. Preserve every attempt. |
| Node view | Derived `status: open \| running \| accepted \| rejected \| failed \| gated \| cancelled`; current attempt, blocking reason, pending gate. Workers cannot write acceptance. |

`acceptance` pins the claim, required checks, verifier version/digest, and policy. Result shape alone does not prove behavior. `failed` means execution/infrastructure failure; `rejected` means a completed checker rejection.

**Evidence ports: pull declared inputs and bind exact references.**

Each port declares its type, requiredness, allowed sources/scope, and handling policy. Resolved evidence records version/digest, subject, provenance/attestation, and applicable validity. Digests identify bytes; they do not establish trusted origin.

| Port | Evidence and boundary |
| --- | --- |
| `rag` | Admitted corpus/index snapshot + scoped query trace + exact document/chunk/span IDs and digests. The retriever enforces permissions before result/model exposure. Preserve “no Nation/unattested Indigenous”; unclear classification blocks admission. |
| `logs` | Run/check/artifact IDs + digests and tested subject. Shareable, access-controlled references; no secrets. |
| `code` | Repository identity + full SHA/tree/patch digest; PR reference if available. A PR URL alone is insufficient. |
| `proofs` | Exact accepted node revision + receipt + typed output. In-graph proof inputs must also be dependencies. |
| `human` | Gate packet and authenticated decision. Basho decides; operator delegation must be explicit and scoped. |

Validate required inputs before dispatch and freeze the attempt's manifest. For RAG, this pins the permitted corpus and role scope; exact slices retrieved within it are append-only produced evidence, with the subset supplied to each model call recorded. Seal that trace before checking. Missing or inadmissible evidence cannot become a pass.

**Verifier.** Input: pinned contract + candidate + input/produced-evidence manifests + dependency receipts. Output: a bound receipt with `accept`, `reject(reason, retry_hint?)`, or `undecidable(reason)`, verifier identity/version, policy, subject, time, and context digest. The controller validates provenance and bindings before recording status. A crash is an operational failure, not a verdict.

Software acceptance means the named checks passed for the named subject. Cyber/regulatory acceptance covers only the attested checklist’s stated scope. Chat cannot upgrade either claim.

**Workflow.**

1. Dispatch only ready open nodes: exact dependencies accepted and admissible, required inputs available, budget/resources available. Blocked dependents remain open with an explicit reason; no speculative execution.
2. Execute, seal the candidate, check, then record the bound verdict.
3. Retry within the original acceptance contract. Narrow repair instructions or optional access, never the promised result or mandatory checks. A smaller claim needs an explicit revision/subtask; the original obligation remains.
4. Apply finite attempts and a total budget. Proposed default: three attempts including the first; exhaustion → Basho gate or cancellation. Replanning does not silently reset budgets.
5. Undecidable → human gate referencing the sealed attempt as context. It must not depend on that paused node being accepted. Basho can authorize retry/revision/cancellation; approval cannot manufacture a machine pass.
6. Build the foundations with one worker; the first complete pilot adds at most two concurrent read-only research workers. Dependency independence, compatible resources, exclusive attempt ownership, and shared budget reservations are mandatory. Writes and verifier execution remain exclusive.
7. A `sink` assembles required receipts into a typed packet without strengthening their claims. Completion requires every declared target; partial output stays labelled partial.

Authority policy applies before dispatch, not only during `gated` status. Send, merge, secret access/use, pay, and delete remain Basho’s decisions, bound to the exact action and subject. Accepted work grants no execution authority.

**Product boundary.** Checkspan owns its work definitions, graph state, attempts, acceptance receipts, and review packets. It is a standalone product with no assumed predecessor, host product, or integration dependency. External tools can participate through documented contracts; any future integration must identify who owns execution authority and each receipt. Handoffs use resolvable IDs, revisions, and structured payloads.

**Multi-agent RAG, required in the proposed first pilot.** The controller dispatches two model-backed specialists: requirements retrieval and implementation retrieval. Each has a separate context, declared corpus scope, and typed result. Synthesis joins both accepted artifacts; a challenger then searches for counterevidence; a deterministic checker validates source membership, spans, digests, required fields, and review disposition. Accepted artifacts form shared memory. Agents cannot broaden source access or mark work accepted.

Agent agreement and valid citations do not prove a factual claim. Model assessments, operator reviews, unresolved contradictions, and insufficient evidence remain distinct. Required unresolved semantic judgments go to a human gate. Corpus attestation establishes permitted provenance/use, not factual truth.

**Smallest complete pilot, still paper-only.** A standalone Rust `checkspan` CLI with SQLite/FTS5 over admitted text snapshots, one approved existing model adapter, four agent roles with a two-reader concurrency cap, one software verifier, bounded calls/retries, and a typed review packet. First path: request + corpus + supplied patch → cited research + software checks → packet. Model/corpus access and usage require scoped approval; embeddings, arbitrary crawling, distributed agents, and automatic patch generation remain later options. Local checks and hosted CI remain distinct evidence.

The [PSPR Draft 0.2](PLANNING/CHECKSPAN-PSPR.md) adds CS-R01–CS-R12 before first-pilot acceptance at CS-25. Actual retrieval, model calls, failure cases, and a single-agent comparison are required; no quality or multi-agent benefit is claimed from this paper.

**Excluded.** A chat wrapper, unconnected “graph” nodes, house-ops automation as product scope, inherited product integrations, invented Prove2Me internals/benchmarks, and Lean-level software claims.
