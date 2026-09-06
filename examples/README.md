# Checkspan examples

Documents for the two read-only commands. Neither command dispatches work, creates a run store, or writes anywhere; each reads one file and prints one JSON object to stdout.

```bash
checkspan validate examples/graphs/review-pilot.json
checkspan inspect examples/graphs/research-pilot.json
```

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | The document is valid (and, for a graph, admitted). |
| 1 | The document was rejected; `diagnostics` says why. |
| 2 | Usage error (unknown command or flag). |
| 3 | The file could not be read. |

## Output

Every response carries `command`, `path`, `valid`, `record`, `schema_version`, `schema_id`, and `diagnostics`. Each diagnostic has a `stage`, a JSON-pointer `path`, and a `message`. Stages, in pipeline order: `size`, `syntax`, `depth`, `duplicate_key`, `header`, `schema`, `shape`, `contract`, `admission`; `io` is reported when the file cannot be read. The first failing stage stops the pipeline.

`inspect` adds `graph` for a graph document (nodes, targets, dependency `order`, the `required` closure of the targets, `optional` nodes no target depends on, and human `gates` with the nodes they resolve) or `identity` for any other record. Keys are emitted in sorted order, so output is stable across runs.

## Graphs

| File | What it shows | Expected |
| --- | --- | --- |
| `graphs/review-pilot.json` | Supplied patch → software checks → review packet, with a human gate that resolves exhausted check attempts and depends only on the patch. | exit 0; order `cs_patch, cs_ci, cs_ci_gate, cs_packet`; the gate is optional. |
| `graphs/research-pilot.json` | Two retrieval tasks → synthesis → challenge → evidence check → packet: a diamond with `proofs` ports that name their in-graph sources. | exit 0; six required nodes, nothing optional. |
| `records/graph-run.json` | A run record, for `inspect` on a non-graph document. | exit 0; `identity.run_id` is `run-0001`. |

## Rejected documents

| File | Stage | Why |
| --- | --- | --- |
| `invalid/cycle.json` | `admission` | `cs_patch` depends on `cs_ci`, which depends on `cs_patch`. |
| `invalid/unresolved-dependency.json` | `admission` | `cs_ci` depends on a node that is not in the graph. |
| `invalid/incompatible-type.json` | `admission` | `cs_ci` expects `software_check_result@1` from `cs_patch`, which produces `patch_result@1`. |
| `invalid/hidden-proof-dependency.json` | `admission` | A `proofs` port names `node:cs_ci_gate` without a dependency on it. |
| `invalid/gate-wait-cycle.json` | `admission` | The gate depends on the node it resolves. |
| `invalid/external-import.json` | `admission` | A dependency names another graph; imports are not supported. |
| `invalid/duplicate-key.json` | `duplicate_key` | `graph_id` appears twice in one object. |
| `invalid/unknown-version.json` | `header` | `schema_version` 2 is not supported by this build. |
| `invalid/bad-identifier.json` | `schema` | `graph_id` contains spaces and capitals. |

## Scope conventions

An evidence port of kind `proofs` names in-graph sources as `node:<node_id>`; each must also be a declared dependency. A `human` port on a `human_gate` lists who may decide as `signer:<identity>` and which nodes' attempts the gate resolves as `node:<node_id>`; a gate and a node it resolves must not be joined by an acceptance chain in either direction.
