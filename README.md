# Viberaven

Viberaven is an evidence-backed catalogue and research archive for current products and tools.
It stores source provenance and reviewed assessments locally, with a headless Rust library and command-line interface.

## Capabilities

- Ingest one explicitly supplied local `.txt`, `.md`, `.rst`, or `.html` file, or a public HTTPS page; inputs are limited to 5 MiB.
- Preserve source identity, publication and retrieval metadata, content hashes, extracted text, and traceable evidence segments.
- Deduplicate exact content and search evidence locally with SQLite FTS5.
- Record draft, approved, or rejected product/version assessments; approved assessments require a decision, reviewer, date, and evidence.
- Preview deterministic README or CSV exports, then apply them only to an explicit destination after checking its fingerprint; README output updates a managed block and preserves surrounding content.
- Back up an export target before applying and support rollback while the target still matches the applied content.
- Schedule bounded HTTPS refreshes and run them once from the CLI; changed content creates a pending review proposal, while prior evidence remains available.
- Record source-change review decisions without inferring product identity, supersession, or export changes.
- No background service starts automatically.

Web retrieval uses HTTPS only, checks resolved addresses, rejects redirects and credential-like URL query parameters, and applies time and size limits.
Tests use offline fixtures and do not contact external sources.

## Requirements

Rust 1.88 or newer and Cargo are required.
The first build needs the pinned crates in `Cargo.lock`; after fetching them, build and tests can run offline.

On this machine, Rust is installed in `%USERPROFILE%\.cargo\bin`, which may need to be added to the current PowerShell session's path.

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

## Run

```powershell
cargo run -- --help
```

The CLI supports `ingest`, `search`, `assessment`, `export`, `refresh`, and `rebuild-search` commands, including `refresh changes list` and `refresh changes review` for source-change proposals.
Run `cargo run -- --help` for the command list and option summary.
The default database is `%LOCALAPPDATA%\Viberaven\catalogue.sqlite3`; `--db PATH` selects another location.

## Check

```powershell
cargo fmt --all -- --check
```

```powershell
cargo clippy --all-targets -- -D warnings
```

```powershell
cargo test
```

## Layout

```text
.
├── .agents/skills/viberaven-project-board/SKILL.md
├── docs/agents/             # Issue tracking, triage labels, and domain guidance
├── docs/research/           # GitHub Projects CLI skill research
├── src/
│   ├── cli.rs               # Non-interactive CLI
│   ├── export.rs            # Deterministic preview, apply, backup, and rollback
│   ├── ingest.rs            # Bounded local and HTTPS source retrieval
│   ├── refresh.rs           # One-shot refresh job runner
│   ├── store.rs             # SQLite provenance, evidence, assessments, and jobs
│   ├── lib.rs               # Public library modules and CLI exports
│   └── main.rs              # CLI entry point
├── tests/                   # Offline workflow and recovery coverage
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── PLAN.md
├── README.md
└── STACK.md
```

Read [PLAN.md](PLAN.md) for scope and the current checkpoint, and [STACK.md](STACK.md) for dependency and resource decisions.
Read [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md) for GitHub Issues and Project configuration.
Use [.agents/skills/viberaven-project-board/SKILL.md](.agents/skills/viberaven-project-board/SKILL.md) for browserless board operations.
