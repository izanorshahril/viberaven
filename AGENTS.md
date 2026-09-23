# Viberaven agent guidance

- Read [PLAN.md](PLAN.md) before choosing work scope or resuming a phase; read [STACK.md](STACK.md) before runtime, dependency, cost, or model decisions.
- Complete one requested phase at a time and update the checkpoint in `PLAN.md` with changed files, checks, decisions, and remaining work.
- Keep the core headless and offline by default; exact-pin dependencies when a scoped capability needs them.
- Keep hosted services, model downloads, resident model processes, and Farseer integration behind an explicitly requested phase with stated cost and resource limits.
- Limit adjacent-repository edits to work explicitly scoped for that repository, and read its current instructions and contracts before changing it.
- Run the Rust checks listed in `PLAN.md` before completing a code phase.
