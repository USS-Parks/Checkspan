# Checkspan M1 — contract explorer guide

M1 is the offline contract explorer: a command-line tool that validates Checkspan records and inspects graphs without running anything. It has no worker, no run store, no credentials, and no network access. If you can build a Rust program, you can reproduce everything in this guide in a few minutes.

## What you need

- Rust via `rustup` (the repository pins toolchain 1.98.0 in `rust-toolchain.toml`; `rustup` installs it on first use).
- Git.
- Windows x64 with the MSVC Build Tools linker, or Linux x64 with a C toolchain (`build-essential` or equivalent).

No account, token, or service is involved.

## Build

```bash
git clone https://github.com/USS-Parks/Checkspan.git
```

```bash
cd Checkspan && cargo build --release --locked
```

The binary is `target/release/checkspan` (`checkspan.exe` on Windows). `--locked` makes Cargo use the committed `Cargo.lock` exactly; if it refuses, the checkout is not the reviewed one.

To run the whole test suite instead of trusting the binary:

```bash
cargo test --locked
```

## Try it

Validate a graph:

```bash
target/release/checkspan validate examples/graphs/review-pilot.json
```

Inspect it:

```bash
target/release/checkspan inspect examples/graphs/review-pilot.json
```

Reject one:

```bash
target/release/checkspan validate examples/invalid/cycle.json
```

The exit code tells you the outcome before you read anything: 0 valid, 1 rejected, 2 usage error, 3 the file could not be read. The JSON on stdout says why. [examples/README.md](../examples/README.md) lists every example with its expected result.

## Reading the output

Every response has the same top-level keys: `command`, `path`, `valid`, `record`, `schema_version`, `schema_id`, and `diagnostics`. A diagnostic has a `stage`, a JSON-pointer `path` into the document, and a `message`.

Stages run in a fixed order and the first failure stops the pipeline: `size` (over 4 MiB), `syntax` (not UTF-8 JSON, or trailing content), `depth` (more than 64 nested containers), `duplicate_key`, `header` (no supported `record`/`schema_version`), `schema` (the bundled JSON Schema rejects it), `shape` (the typed record rejects it), `contract` (a rule within the record), `admission` (a rule across the graph's nodes). So a document that fails at `schema` has already passed size, syntax, depth, duplicate-key, and header checks.

`inspect` adds a `graph` section for a graph document: every node with its kind, dependencies, result type, required checks, and attempt cap; the `targets`; the dependency `order` (producers before consumers); the `required` closure of the targets; `optional` nodes no target depends on; and human `gates` with the nodes they resolve. For any other record it adds an `identity` section with the record's natural key.

## What a valid graph means

Passing `validate` establishes that the document is well formed, that every reference resolves to an exact in-graph node and revision, that dependency types match, that there are no cycles, that proof inputs declare their dependencies, and that no human gate waits on the node it resolves. It establishes nothing about whether the work described would succeed. There is no status, verdict, or acceptance anywhere in a graph document, and `inspect` cannot produce one.

## What M1 does not do

- It does not run, schedule, or retry anything; there is no worker.
- It does not create or read a run store; running the commands leaves no files behind.
- It does not fetch schemas or anything else; every `$ref` is served from schemas compiled into the binary, and the build contains no HTTP client.
- It does not accept documents from other graphs or runs; cross-graph dependencies and imported receipts are rejected as unsupported.

## Reproducing the acceptance evidence

`test-evidence/checkspan/CS-07/run-examples.sh` runs every example through a binary and prints the exit code, validity, rejecting stage, and SHA-256 of the output for each. The recorded Windows and Linux runs are beside it; the output digests match across the two platforms.

```bash
bash test-evidence/checkspan/CS-07/run-examples.sh target/release/checkspan /tmp/my-run.txt
```

Compare your file with `windows-x64.txt` or `linux-x64.txt`; only the `binary`, `commit`, and `platform` header lines should differ if you built a different commit or on a different machine.
