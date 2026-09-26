# Viberaven implementation plan

Status: Phase 1 bootstrap, Phase 2 vertical slice, and Phase 4 refresh/export workflows are implemented; Phase 3 needs a separately scoped Farseer bridge extension, and Phase 5 remains gated on an evidenced quality gap and explicit resource approval.
Updated at local date 2026-09-26, Asia/Kuala_Lumpur.
Read this file to resume; read [STACK.md](STACK.md) for stack decisions, exceptions, costs, and memory gates.
These documents do not authorize installation, model/API spending, publishing, or changes to adjacent projects.

## Product

Maintain an evidence-backed catalogue of useful/current products and a reusable research archive for other projects under `D:\Dev`.
Collect explicit web sources and later video/Reddit/platform resources; retain provenance and distinguish source claims from verified assessments.
Support reviewed catalogue changes, scheduled refreshes, supersession history, and deterministic export to `D:\Dev\awesome-vibe-ai`.
Later use the same evidence for guides, workflows, and other content rather than creating another source of truth.

## Current state and references

- Workspace: `D:\Dev\viberaven`; the Rust library and non-interactive CLI implement bounded ingest, local evidence search, human-reviewed assessments, refresh jobs, change proposals, and deterministic export.
- GitHub repository: `izanorshahril/viberaven`, on local branch `main`; the repository issue tracker and [Roadmap Project #2](https://github.com/users/izanorshahril/projects/2) are configured for browserless `gh` operations.
- GitHub auth was verified with the `project` scope; no token values were copied or persisted.
- Issues [#2-#4](https://github.com/izanorshahril/viberaven/issues/2), [#6](https://github.com/izanorshahril/viberaven/issues/6), and [#7](https://github.com/izanorshahril/viberaven/issues/7) are implemented locally; project status reconciliation follows final review and commit.
- Issue [#5](https://github.com/izanorshahril/viberaven/issues/5) needs an explicit Farseer bridge/API extension because its current widget bridge exposes no Viberaven catalogue or review actions.
- Issue [#8](https://github.com/izanorshahril/viberaven/issues/8) remains gated; no deterministic-baseline gap is documented, and no model or inference resources were approved.
- `Cargo.toml` exact-pins the direct Rust dependencies, and `Cargo.lock` pins their transitive versions.
- `D:\Dev\awesome-vibe-ai` and `D:\Dev\farseer` were inspected but not modified; their existing files and contracts remain separate.
- Local `D:\Dev\vibe-cat` is absent, but the public [vibe-cat prototype](https://github.com/izanorshahril/vibe-cat) remains available; preserve useful behavior, not its Python implementation.
- [yt-research](https://github.com/izanorshahril/yt-research) provides transcript/source identity and export references; it is not the new runtime.
- `D:\Dev\awesome-vibe-ai` contains the README/CSV and untracked `Inspirationst.txt`; its listings are discovery leads, not current factual authority; preserve unrelated edits.
- `D:\Dev\farseer` has an actively modified Rust/React workspace; consult its current `AGENTS.md`, `CORE.md`, and widget contract before integration; never overwrite its uncommitted work.

## Architecture boundaries

One Rust package initially, with a headless library and thin CLI/service entry points.
Use modules for source ingestion, evidence, assessments, persistence, and export; add interfaces only at real external boundaries.
SQLite is canonical for Viberaven's evidence metadata, products, assessments, and durable jobs.
Large permitted artifacts can live as content-addressed local files with hashes/references in SQLite and an explicit retention policy.
Indexes and summaries are derived data, not alternate canonical stores.

| Boundary | Contract |
| --- | --- |
| Source | Bounded document with source ID/URL, publication/retrieval timestamps, hash, text/segments, and extraction status |
| Evidence | Traceable excerpts/claims with source references; preserve contradictions and unknown values |
| Product/version | Stable identity, vendor, aliases, release/version, explicit supersession relationships |
| Assessment | Use case, constraints, criteria, evidence, decision date, reviewer, review state; not a universal `is_sota` boolean |
| Job | Durable state, attempts, ownership/lease, next run, cancellation; idempotent resume after interruption |
| Export | Deterministic preview followed by separately authorized application; never silently overwrite another repository |

Missing price is unknown, not free; missing hardware requirements are unknown, not proof of fit.
A newer release does not automatically supersede every older product or use case.
Outdated products leave the active view but remain in history with the reason and evidence.
Keep publication, retrieval, release, and last-verification dates distinct.

## Farseer integration

Widgets render a cell's work and ask the top manager for actions; they do not directly address a cell or run another backend inside the frame.
The sandbox permits React and widget-local imports, with `farseer.read`, `farseer.ask`, and namespaced UI state; it does not give widgets credentials, arbitrary imports, or direct network access.
Verify the live contract at `D:\Dev\farseer\widgets\AGENTS.md` and its implementation before coding.

- Keep Viberaven independently usable with its own database; research logic stays outside Farseer's pure core.
- Add a narrow Farseer application/API adapter for projections only in a separately approved integration phase; no such adapter exists yet.
- Reads use the sanctioned widget bridge; control requests use the top manager's authority path.
- Prefer CLI invocation for the first concrete command integration; add authenticated loopback HTTP/SSE when live projections need it.
- Preserve Farseer's orchestration record; do not duplicate its canonical run store or invent a dynamic plugin ABI.

## Delivery sequence

Complete one requested phase at a time and record evidence in the checkpoint below.
The workspace is past Phase 1; this roadmap is not a request to implement later phases automatically.

| Phase | Bounded work | Completion evidence |
| --- | --- | --- |
| 1. Bootstrap | Inspect current files/rules; initialize Git if absent; create minimal Rust package and concise README/AGENTS guidance; configure installed engineering skills only as needed; pin dependencies when first used | Build and one runnable test pass; CLI help works; no resident services, model loads, paid calls, or adjacent-repo changes |
| 2. Vertical slice | Explicit URL or permitted local document; provenance; exact dedupe; FTS5 search; human-reviewed product/version assessment; export preview | One E2E fixture covers ingestion through review/export preview; malformed input, duplicates, and interruption tested |
| 3. UI and Farseer | Evidence-first catalogue/detail/review UI; bridge adapter in a separately scoped Farseer change | Real widget reads through bridge; controls follow authority; keyboard, responsive, loading, empty, error, and undo states verified |
| 4. Refresh/export | Durable scheduled jobs, source limits, reviewed supersession, scoped README/CSV exporter | Restart without duplicate publication; deterministic diff; concurrent-edit detection; backup/restore and rollback verified |
| 5. Optional AI | Evaluate rules against one small GGUF; consider Jev only after approved budget and demonstrated local gap | Held-out quality/calibration, latency, memory, and cost recorded; shared-machine limits pass; no automatic destructive decisions |

Use Windows Task Scheduler to invoke the eventual CLI before adding an always-on scheduler service.
Import prototype/catalogue data through dry run, explicit field mapping, provenance retention, and deduplication; a prototype's claim is not newly verified evidence.
Connectors must report unsupported/inaccessible transcripts clearly instead of fabricating content or repeatedly retrying a blocked endpoint.

## Safety and quality gates

1. Validate external schemas, URLs, and paths; restrict protocols/redirects; prevent untrusted fetch targets from reaching private/link-local/loopback networks while separately allowing explicitly configured local model endpoints.
2. Treat scraped content as untrusted data, never executable instructions; keep keys, browser credentials, and local files outside scraped/model-directed access; redact secrets from logs.
3. Respect permitted platform access and retention terms; bound concurrency, response size/time, retries, and disk retention; support cancellation; do not bypass access controls.
4. Require human review for publication, uncertain identity merges, supersession, and active-view removal; retain historical evidence and rollback data.
5. Test offline by default; live provider calls, model loads, adjacent-repo writes, and paid integrations need explicit scope/budget; preserve unrelated work.

Rust gates after implementation: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
For Farseer edits, use its current workspace/UI gates rather than Viberaven-only tests.
Use Bun for UI scripts and review lifecycle scripts before allowing them during installation.
Validate packaged Windows behavior, not only a dev server.
Do not claim E2E UI acceptance from a unit test or an unobserved screenshot.

## AI evaluation gate

Begin with a small held-out set of real tasks; expand toward 100-200 labelled examples when considering a model.
Cover changing categories, multilingual input, duplicates, missing facts, misleading announcements, contradictions, and superseded versions.
Measure precision/recall, abstention/calibration, retrieval quality where relevant, cold/warm latency, memory pressure, and actual hosted spend.
Model-authored confidence or game demos do not replace research-domain validation.
Apply [STACK.md](STACK.md)'s resource gates; no model is approved for download or residency yet.

## Resume checkpoint

| Item | State |
| --- | --- |
| Completed | Phase 1 bootstrap; Phase 2 ingest/search/review/export-preview slice; Phase 4 durable refresh, review proposals, explicit export/apply/rollback; GitHub Project skill research and repo-local board skill |
| Changed files | `Cargo.toml`, `Cargo.lock`, `src/{cli,export,ingest,lib,main,refresh,store}.rs`, `tests/{refresh_state,vertical_slice}.rs`, `README.md`, `PLAN.md`, `STACK.md`, and Clippy cleanup; removed obsolete `HANDOFF.md` |
| Checks | `cargo fmt --all -- --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, and `cargo test --locked --offline` pass; 12 tests pass, with CLI help/version smoke checks also passing |
| Storage and networking | SQLite is bundled with FTS5; inputs and network work are bounded; default operation is offline, and no background service starts |
| Model and hosted calls | No model downloads or inference, no paid calls, and no external project writes |
| GitHub planning | Live issue specs #2-#8 were read; #2, #3, #4, #6, and #7 are implemented locally; board statuses remain to be reconciled after review |
| Remaining implementation | #5 requires explicit scope for a Farseer bridge/API change; #8 requires a documented deterministic gap and explicit approval for model/resource use |
| Next action | Commit the reviewed README export fix, reconcile completed issue statuses in Project #2, then request only the Farseer scope and semantic-resource decisions needed for #5 and #8 |

New session: read this plan, inspect the actual workspace and applicable instructions, then continue only with the requested phase or decision.
Update this checkpoint with changed files, checks actually run, decisions, and remaining work instead of creating duplicate handoff files.
