# Viberaven agent guidance

- Read [PLAN.md](PLAN.md) before choosing work scope or resuming a phase; read [STACK.md](STACK.md) before runtime, dependency, cost, or model decisions.
- Complete one requested phase at a time and update the checkpoint in `PLAN.md` with changed files, checks, decisions, and remaining work.
- Keep the core headless and offline by default; exact-pin dependencies when a scoped capability needs them.
- Keep hosted services, model downloads, resident model processes, and Farseer integration behind an explicitly requested phase with stated cost and resource limits.
- Limit adjacent-repository edits to work explicitly scoped for that repository, and read its current instructions and contracts before changing it.
- Run the Rust checks listed in `PLAN.md` before completing a code phase.

## Agent skills

### Issue tracker

Track specifications and implementation tickets in GitHub Issues; use the Viberaven Roadmap Project for planning status after it is linked. See `docs/agents/issue-tracker.md`.

### Project board operations

Use [.agents/skills/viberaven-project-board/SKILL.md](.agents/skills/viberaven-project-board/SKILL.md) when creating or linking the Roadmap Project, creating or placing issues on it, reviewing board coverage, or updating Project fields; the skill reads `docs/agents/issue-tracker.md` for configuration.

### Triage labels

Map Matt Pocock's five canonical triage roles to this repository's label names. See `docs/agents/triage-labels.md`.

### Domain docs

Use the single-context layout for domain language and architecture decisions. See `docs/agents/domain.md`.
