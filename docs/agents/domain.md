# Domain docs

Read these files when exploring a domain area:

- Read `CONTEXT.md` at the repository root when it exists.
- Read `CONTEXT-MAP.md` instead when present, then open the relevant context glossary.
- Read relevant architecture decisions under `docs/adr/`.
- In a multi-context layout, also read relevant `src/<context>/docs/adr/` decisions.

If these files do not exist, continue without flagging their absence; create them when domain terms or durable architecture decisions need recording.

Use the glossary's terms in issue titles, implementation plans, tests, and reviews.

Surface a conflict with an existing architecture decision instead of silently overriding it.

This repository uses the single-context layout: one root `CONTEXT.md` and system-wide decisions in `docs/adr/`.
