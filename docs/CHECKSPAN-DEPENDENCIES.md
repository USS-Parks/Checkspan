# Checkspan — dependency and toolchain decision record

Maintained per implementation prompt. Every entry names the exact version, license, why it is used, and what was considered instead. Library availability is not proof of suitability; each later prompt rechecks maintenance, licenses, advisories, and supported targets when it starts.

## Toolchain (CS-01)

| Item | Pinned value | Where |
| --- | --- | --- |
| Rust toolchain | `1.98.0` stable, components `rustfmt` + `clippy`, profile `minimal` | `rust-toolchain.toml` |
| Edition / MSRV | 2024 / `rust-version = "1.98"` | `Cargo.toml` |
| Lockfile | `Cargo.lock` committed; all CI and local gates run `--locked` | repository root |
| Supported targets | `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu` | `deny.toml` `[graph].targets`, CI matrix |
| Native prerequisites | Windows: MSVC Build Tools linker (verified present on the development machine). Linux: system C toolchain provided by the `ubuntu-24.04` runner image. Git for source identity. | recorded here; not assumed elsewhere |
| Package layout | One package, `checkspan` library plus thin `checkspan` binary. A second package is added only when a real dependency or publication boundary requires it. | `Cargo.toml` |
| Publication | `publish = false`; distribution terms are settled before any public binary or registry publication. | `Cargo.toml`, `deny.toml` `[licenses.private]` |

Lints: `unsafe_code = "forbid"` package-wide; `missing_docs = "warn"` on the library crate only. Clippy runs with `-D warnings` so both are enforced at the gate.

## Dependencies

| Crate | Version | License | Purpose | Alternatives considered | Added in |
| --- | --- | --- | --- | --- | --- |
| `clap` (features: `derive`) | 4.6.6 | MIT OR Apache-2.0 | Command-line parsing with generated help/version and stable exit codes | Hand-rolled argument parsing (rejected: help, errors, and exit-code conventions would be re-implemented), `argh`/`lexopt` (smaller, but the PSPR names `clap` and its derive API keeps subcommands declarative) | CS-01 |
| `serde` (features: `derive`) | 1.0.229 | MIT OR Apache-2.0 | Typed contract records with `deny_unknown_fields` and `try_from` newtypes | Hand-written JSON mapping (rejected: every record would repeat the same strictness code) | CS-02 |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | JSON parsing to `Value` for header-first record dispatch, then typed deserialization | `json` crate (rejected: no serde integration) | CS-02 |
| `jsonschema` (**dev-dependency**, `default-features = false`) | 0.54.0 | MIT | Draft 2020-12 validation of fixtures against the bundled schemas in tests; no HTTP, file, or TLS resolver is compiled in | `boon` (rejected: smaller ecosystem; `jsonschema` is the PSPR default). Promotion to a runtime dependency is decided by the offline validation prompt. | CS-02 |

Transitive closure at CS-02: 90 crates (`cargo tree --edges normal,dev`), all MIT, Apache-2.0, Unicode-3.0, or Unlicense-OR-MIT (`cargo deny check licenses`). `cargo audit`: no advisories. No git or non-crates.io sources.

CS-03: no dependency change.

Schema `$id`s use `https://checkspan.invalid/schemas/v1/`. `.invalid` is reserved by RFC 2606 and never resolves, which makes the IDs identifiers rather than fetchable locations; the bundled registry is the only source of schema text. Changing the ID base is a schema-version change.

## Verification tooling

| Tool | Version observed | Gate | Runs where |
| --- | --- | --- | --- |
| `cargo fmt` | rustfmt 1.9.0-stable | format | local pre-commit ladder, CI |
| `cargo clippy` | 1.98.0 | lint, warnings as errors | local, CI |
| `cargo test --locked` | 1.98.0 | unit + integration | local, CI |
| `cargo build --locked --release` | 1.98.0 | build/smoke | local, CI |
| `cargo deny check` | cargo-deny 0.19.7 | licenses, advisories, bans, sources | local per dependency change; configuration in `deny.toml` |
| `cargo audit` | cargo-audit 0.22.1 | RustSec advisories | local per dependency change |

CI (`.github/workflows/ci.yml`) is limited to `contents: read`, uses one third-party action (`actions/checkout`, pinned by commit SHA), installs the toolchain from `rust-toolchain.toml` through the runner's preinstalled `rustup`, and serializes runs per ref with `cancel-in-progress: false`. It runs on pushes to `main` and `codex/**` and on pull requests.

## Planned additions (not yet decided)

These are the PSPR's proposed defaults for later prompts. Each is chosen, versioned, and reviewed only in the prompt that introduces it: `jsonschema` as a runtime dependency with bounded parsing (CS-05), SHA-256 and RFC 8785 canonicalization (CS-08), `rusqlite` with bundled SQLite (CS-09).
