# Checkspan — product one-pager v0.2

**Standalone product exploration · paper only · no build authorization**

*Work you can verify.*  
**Canonical repository:** [USS-Parks/Checkspan](https://github.com/USS-Parks/Checkspan)  
**Working folder:** `C:\Users\17076\Documents\Codex\Work Graph Project`  
**Execution plan:** [Canonical PSPR — draft for review](PLANNING/CHECKSPAN-PSPR.md). Implementation requires explicit STS approval.  
**Revision:** 2026-09-06; establishes Checkspan as an independent product and carries forward the reviewed schema. See [schema notes and review cases](CHECKSPAN-SCHEMA-NOTES.md).

**Product.** Checkspan organizes AI-assisted work into connected tasks, checked results, and explicit human decisions. The name combines the check at each acceptance boundary with the span of work across dependencies.

**First customer problem.** A developer reviewing an AI-produced patch needs to know what changed, which checks actually ran, which exact candidate they checked, and what still needs a decision. Checkspan assembles those answers into a traceable review packet.

**Thesis.** Work becomes a DAG of typed claims and artifacts. An immutable node contract declares intent, inputs, and acceptance criteria. A worker produces a candidate; a designated checker records a verdict against that exact candidate and evidence. Acceptance establishes only the stated contract. Authorization to act is separate.

**Public precedent.** Anthropic describes using Prove2Me, developed by Tianyi Peng and collaborators at Columbia, to coordinate theorem dependencies and parallel agents. Prove2Me documents open/proved states and Lean checking. These public patterns motivate the design; they do not make software tests equivalent to mathematical proof. [Anthropic account](https://www.anthropic.com/research/formalizing-fermats-last-theorem), [Prove2Me explanation](https://prove2.me/about).

**Schema: keep the contract separate from execution history.**

| Record | Contents |
| --- | --- |
| Graph | `schema_version`, `graph_id`, `revision`, `nodes[]`, required `targets[]`, total budget. Validate resolved references, unique IDs, acyclicity, compatible types, and target coverage. |
| Node contract | Namespaced `id` + `revision`; `kind: task \| check \| human_gate \| sink`; intent `prompt`; typed `deps[]`; `evidence_ports[]`; versioned `result_type`; pinned `acceptance`; retry and authority policies. Immutable during a run. |
| Attempt | Run/node/attempt IDs, owner, timestamps, actual input manifest, exact dependency receipts, candidate result/digest, produced evidence, execution outcome, verifier receipt. Preserve every attempt. |
| Node view | Derived `status: open \| running \| accepted \| rejected \| failed \| gated \| cancelled`; current attempt, blocking reason, pending gate. Workers cannot write acceptance. |

`acceptance` pins the claim, required checks, verifier version/digest, and policy. Result shape alone does not prove behavior. `failed` means execution/infrastructure failure; `rejected` means a completed checker rejection.

**Evidence ports: pull declared inputs and bind exact references.**

Each port declares its type, requiredness, allowed sources/scope, and handling policy. Resolved evidence records version/digest, subject, provenance/attestation, and applicable validity. Digests identify bytes; they do not establish trusted origin.

| Port | Evidence and boundary |
| --- | --- |
| `rag` | Attested corpus version + slice IDs/digests. Preserve “no Nation/unattested Indigenous”; unclear classification blocks retrieval pending Basho’s decision. |
| `logs` | Run/check/artifact IDs + digests and tested subject. Shareable, access-controlled references; no secrets. |
| `code` | Repository identity + full SHA/tree/patch digest; PR reference if available. A PR URL alone is insufficient. |
| `proofs` | Exact accepted node revision + receipt + typed output. In-graph proof inputs must also be dependencies. |
| `human` | Gate packet and authenticated decision. Basho decides; operator delegation must be explicit and scoped. |

Validate required inputs before dispatch and freeze the attempt’s manifest. Separately seal produced evidence before checking. Missing or inadmissible evidence cannot become a pass.

**Verifier.** Input: pinned contract + candidate + input/produced-evidence manifests + dependency receipts. Output: a bound receipt with `accept`, `reject(reason, retry_hint?)`, or `undecidable(reason)`, verifier identity/version, policy, subject, time, and context digest. The controller validates provenance and bindings before recording status. A crash is an operational failure, not a verdict.

Software acceptance means the named checks passed for the named subject. Cyber/regulatory acceptance covers only the attested checklist’s stated scope. Chat cannot upgrade either claim.

**Workflow.**

1. Dispatch only ready open nodes: exact dependencies accepted and admissible, required inputs available, budget/resources available. Blocked dependents remain open with an explicit reason; no speculative execution.
2. Execute, seal the candidate, check, then record the bound verdict.
3. Retry within the original acceptance contract. Narrow repair instructions or optional access, never the promised result or mandatory checks. A smaller claim needs an explicit revision/subtask; the original obligation remains.
4. Apply finite attempts and a total budget. Proposed default: three attempts including the first; exhaustion → Basho gate or cancellation. Replanning does not silently reset budgets.
5. Undecidable → human gate referencing the sealed attempt as context. It must not depend on that paused node being accepted. Basho can authorize retry/revision/cancellation; approval cannot manufacture a machine pass.
6. Start with one worker. Later parallelism requires dependency independence, compatible resource access, and exclusive attempt ownership.
7. A `sink` assembles required receipts into a typed packet without strengthening their claims. Completion requires every declared target; partial output stays labelled partial.

Authority policy applies before dispatch, not only during `gated` status. Send, merge, secret access/use, pay, and delete remain Basho’s decisions, bound to the exact action and subject. Accepted work grants no execution authority.

**Product boundary.** Checkspan owns its work definitions, graph state, attempts, acceptance receipts, and review packets. It is a standalone product with no assumed predecessor, host product, or integration dependency. External tools can participate through documented contracts; any future integration must identify who owns execution authority and each receipt. Handoffs use resolvable IDs, revisions, and structured payloads.

**Smallest deployable option, still paper-only.** A standalone `checkspan` CLI: DAG JSON, one worker, one software verifier plugin, bounded retries, and a review packet. First path: patch artifact → CI check → packet sink, with exact candidate and trusted check definitions. Local checks and hosted CI remain distinct evidence. No merge/send executor.

**Excluded.** A chat wrapper, unconnected “graph” nodes, house-ops automation as product scope, inherited product integrations, invented Prove2Me internals/benchmarks, and Lean-level software claims.
