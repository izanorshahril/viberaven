# Viberaven

Viberaven is an evidence-backed catalogue and reusable research archive project for current products and tools.

The workspace is bootstrapped with a Rust library and CLI; ingestion, persistence, search, review, and export are not implemented yet.

The bootstrap has no external Rust dependencies and its CLI works offline.

The package requires Rust 1.85 or newer and Cargo.

On this machine, Rust is installed in `%USERPROFILE%\.cargo\bin`, which is not on the initial PowerShell PATH.

To expose it in the current PowerShell session, run:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

## Run

```powershell
cargo run -- --help
```

```powershell
cargo run -- --version
```

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
├── src/
│   ├── lib.rs       # CLI argument parsing and help text
│   └── main.rs      # CLI entry point
├── .gitignore
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── docs/
│   └── agents/
│       ├── domain.md
│       ├── issue-tracker.md
│       └── triage-labels.md
├── HANDOFF.md
├── PLAN.md
├── README.md
└── STACK.md
```

Read [PLAN.md](PLAN.md) for the phased delivery sequence and [STACK.md](STACK.md) for dependency, cost, and resource decisions.

Read [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md) for the GitHub Issues and Project workflow.
