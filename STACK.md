# Viberaven stack shortlist

Status: Phase 1 bootstrap exists with no external Rust dependencies; no model download or inference performed.
Updated at local date 2026-09-23, Asia/Kuala_Lumpur.
Most repository metrics were checked on 2026-09-21; llama.cpp and local model inventory were checked on local date 2026-09-22.
Start implementation from [PLAN.md](PLAN.md); this file owns selection, exclusions, costs, and resource limits.

## Decision

Build a headless Rust application with SQLite and a small React/TypeScript face inside Farseer.
Default operation must require no paid API, running model, GPU allocation, or Python application runtime.
Ingestion, evidence search, human review, catalogue maintenance, and export must work without AI; unattended semantic assessment and synthesis remain optional.
An installed tool does not become a mandatory application dependency.

## Admission policy

- Default screen: at least 5,000 stars, at least 3 identifiable non-bot contributors, and relevant source/release activity within the preceding calendar month at adoption.
- Allow explicit exceptions for established standards, mature libraries, or proven local integrations; stars are a screening aid, not a quality score.
- Keep fresh launches, thin wrappers, and demo-led model clones out until sustained maintenance and task-relevant evidence exist; popularity alone does not establish maturity.
- Prefer zero incremental API spend and no extra resident services; measure footprint and accuracy before accepting a model-backed tool.
- Record exceptions by name and reason; recheck activity, license, advisories, Windows compatibility, and exact versions before installation.

The 5,000-star threshold is a proposed practical interpretation of low stars, not an industry standard.
Contributor counts are paginated GitHub accounts after filtering GitHub bots and recognizable bot/agent usernames, not verified human totals.
GitHub links only the first 500 author email addresses to accounts, so these are lower bounds, especially incomplete for large mature projects.
Monthly activity uses distinct identified non-bot commit authors, not all reviewers or maintainers.
Source: [GitHub contributor-count semantics](https://docs.github.com/en/rest/repos/repos#list-repository-contributors).
No reliable monthly star-growth series was collected.

## Application shortlist

| Component | Stars / identified contributors | Activity snapshot | Setup decision | Cost / footprint boundary |
| --- | ---: | --- | --- | --- |
| Rust + Cargo | Existing toolchain | Rust 1.98.0 observed | One package with library and CLI/service entry points | No service fee; compile on demand |
| [Axum](https://github.com/tokio-rs/axum) | 27,197 / 434 | Sep 21 | HTTP/SSE when the first client needs them | Match Farseer; no second web framework |
| [Tokio](https://github.com/tokio-rs/tokio) | 33,197 / 437 | Sep 17 | Bounded I/O, cancellation, timeouts | Explicit concurrency caps |
| [Reqwest](https://github.com/seanmonstar/reqwest) | 11,833 / 389 | Sep 14 | Source APIs and bounded HTTP retrieval | No paid fetch service by default |
| SQLite + [Rusqlite](https://github.com/rusqlite/rusqlite) | 4,404 / 138 for Rusqlite | Sep 21 | Persistence, durable jobs, initial FTS5 search | No DB server; small-star exception for a mature binding already used by Farseer |
| [Serde](https://github.com/serde-rs/serde) | 10,829 / 180 | Aug 25 | Typed JSON and persisted schemas | No model-backed parsing |
| [Clap](https://github.com/clap-rs/clap) | 16,719 / 412 | Sep 18 | Non-interactive CLI, JSON output, exit codes | No shell framework |
| [React](https://github.com/react/react) | 250,623 / 409 | Sep 18 | Small Farseer widget | Reuse host dependencies |
| [Vite](https://github.com/vitejs/vite) | 82,931 / 430 | Sep 21 | Farseer's existing widget build | Standalone UI build only when requested |

Dates are inspected non-bot commits and can include build/documentation work; they do not certify release stability.
No versions are finalized here; reusing Farseer's stack does not mean copying its older pins without review.
Exact-pin new dependencies and retain their lockfiles; commit only when requested.
Verify FTS5 in the selected SQLite build and keep blocking database work off async request execution.

## Tooling shortlist

| Tool | Stars / contributors; monthly active if measured | State | Cost / resource decision |
| --- | ---: | --- | --- |
| Installed ripgrep + Cargo metadata | Existing tools | Default | Exact search and dependency inventory; no index server or LLM |
| Installed Matt Pocock engineering skills | Existing local collection | On demand | Reuse spec, domain modeling, TDD, diagnosis, review, and writing guidance; do not copy the collection |
| [Impeccable](https://github.com/pbakaus/impeccable) | 69,563 / 47; 12 active | UI-phase candidate | Review engine downloads/hooks; host-agent quota still applies, so usage is not automatically free |
| [ast-grep](https://github.com/ast-grep/ast-grep) | 15,988 / 90 | Optional | Add only when structural queries beat existing search; no resident model |
| [Graphify](https://github.com/Graphify-Labs/graphify) | 120,104 / 256; 60 active | Optional developer tool | Reuse Farseer's project-local code-only pattern; no docs/media model pass, watcher, or global service |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | 129,072 / 445 | Optional inference engine | GGUF inference, explicitly started; no mandatory resident model |
| [9Router](https://github.com/decolua/9router) | 29,512 / 275; 64 active | Existing external infrastructure | Only approved providers; gateway access and model discovery do not prove zero cost |
| [Headroom](https://github.com/headroomlabs-ai/headroom) | 73,370 / 259; 33 active | Existing external infrastructure | Audit features, telemetry, model use, and routing before changes; no blanket extras installation |

Graphify and Headroom may use Python outside the application; neither belongs in Viberaven's deployment dependency chain.
Farseer's mapping recipe is `D:\Dev\farseer\tools\graphify\README.md`.
No plugin registry or background daemon should be created merely for an optional tool.

## Deferred, not part of setup

| Candidate | Decision and reopening condition |
| --- | --- |
| [QMD](https://github.com/tobi/qmd) | 29,924 stars, 82 contributors, 4 monthly active; defer local embedding/query-expansion/reranking models and indexing until retrieval evaluation shows a material FTS5 gap |
| [Serena](https://github.com/oraios/serena) | 29,670 stars, 214 contributors; defer until symbol navigation limits work; check whether Headroom already registered it |
| [Jev](https://docs.typesafe.ai/api) | Paid hosted inference is outside the zero-cost default; an existing key is not budget approval; allow a separately approved capped comparison only if rule/local baselines fail |
| [Cactus Needle](https://github.com/cactus-compute/needle) | 12,018 stars, 29 contributors, 18 monthly active; small native model merits observation but is not a GGUF drop-in and has not passed workload/Windows/resource gates |

Jev probabilities are not factual verification; measure task-level calibration before automated actions.
[Treg's examples](https://treg.to/jev) are useful references for bounded decisions and approved UI variants, not evidence that a paid provider is necessary.

## Removed from setup

- Single/duo contributor experiments: SemIf, Decider, NanoJev, OpenSourceJev, Bespoke Nimble, razorback16/OpenJev, and OpenJev-SGLang.
- `browser-use/jev-ultrafast`: one identified contributor and a Python demo stack; retain the bounded-action idea, not the dependency.
- Laya: three identified contributors but insufficient maturity and validated multilingual calibration for this use; no replacement classifier stack now.
- Spider: 2,727 stars; no compelling exception while the first slice needs only explicit URLs and source APIs; Chromiumoxide also failed the one-month activity gate.
- No extra vector/graph database, orchestration platform, desktop shell, or generative UI framework without a measured missing capability.

These are scope/risk decisions, not claims that the projects are bad or fraudulent.
Reopening requires evidence and an explicit decision, not a popularity spike.

## Shared-machine model policy

Observed hardware: approximately 63 GiB system RAM and an RTX 3070 Laptop GPU with 8,192 MiB dedicated VRAM.
A read-only GPU snapshot showed 1,720 MiB used and 6,299 MiB free; this is not reserved capacity or a forecast for a game/video workload.
No load test or inference was performed.

- `D:\AI\LLM\llama.cpp` exists, but `llama-server`/`llama-cli` were not found in PATH or the inspected checkout; executable location and build compatibility remain unverified.
- Git refused the checkout's ownership; no global trust or ownership settings were changed.
- `D:\AI\Models\protoLabsAI\Ornith-1.0-9B-MTP-GGUF\Ornith-1.0-9B-MTP-Q4_K_M.gguf` is 5,780,090,240 bytes, approximately 5.38 GiB.
- Checkout files named `ggml-vocab-*.gguf` are vocabulary assets, not usable model weights.

The existing 9B file is not selected for background use: its weights alone exceed the small-model budget below.
Its [model card](https://huggingface.co/protoLabsAI/Ornith-1.0-9B-MTP-GGUF) documents a minimum llama.cpp build for MTP; compatibility and working-memory fit remain untested.
GGUF size is not runtime memory: context/KV cache, compute buffers, mappings, and GPU offload change the footprint.
Reference: [llama.cpp server documentation](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).

| Mode | Proposed admission budget, not a benchmark or implemented enforcement |
| --- | --- |
| Background research / gaming / video | No model loaded by Viberaven; zero GPU offload; deterministic ingestion, dedupe, search, and review remain available |
| Explicit small-model trial | One supported GGUF, at most 2 GiB weights, at most 4,096 context tokens, one active request; start CPU-only with at most two inference threads |
| Runtime measurement | Target at most 4 GiB incremental total system-memory pressure and preserve at least 16 GiB available RAM; measure private commit, working set, system availability, and GPU memory |
| Failure | If pressure or responsiveness is unacceptable, stop the Viberaven-owned trial and defer semantic work; never kill/reconfigure the user's other model server |
| Idle | No autostart; unload/stop Viberaven-owned inference after work; no speculative warm residency |

No new model is approved for download.
Prefer an already owned, license-compatible small GGUF only after locating a supported executable and passing an offline task evaluation.
CPU-only inference can still disrupt games/video through CPU, RAM, and disk contention; coexistence testing decides admission.
GPU inference during interactive use remains opt-in with a measured cap; free VRAM is not an allocation target.

## Cost and context discipline

Zero cost means zero incremental hosted service fees, not zero electricity, bandwidth, storage, or subscription usage.
Fail closed on paid routes unless the user sets a spend cap; use manual review or deferred work rather than silent paid fallback.
Cache by content hash plus schema/model/prompt version, deduplicate before inference, and send relevant excerpts with provenance.
Keep code search, archive retrieval, classification, and synthesis separate so one does not require loading all the others.
Store evidence outside prompts, bound search output, and retain full source references when compression is used.
Do not auto-summarize every source, install model bundles, or enable semantic indexing merely because supported.
