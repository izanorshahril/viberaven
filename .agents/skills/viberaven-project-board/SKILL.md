---
name: viberaven-project-board
description: Use when creating or linking the Viberaven Roadmap Project, creating or adding issues, listing board items, changing Project fields, or auditing and reconciling status.
---

# Viberaven Project Board

## Start

1. Read `docs/agents/issue-tracker.md` before any Project operation; proceed with its configured repository, owner, title, number, and issue commands.
2. Confirm `origin` matches the repository in that guide before writing to GitHub; proceed only when they match.
3. Read current Project and issue state before each mutation; proceed when the target and requested change are verified in live state.
4. Before a write, check `gh auth status --hostname github.com`; proceed only with the `project` scope, refreshing it with `gh auth refresh --hostname github.com --scopes project` when missing.

## Create or Link the Roadmap

When the user asks to create or set up the Roadmap, run `gh project list --owner <configured-owner>` and look for the exact configured title before creating anything.

- If exactly one matching Project exists, reuse it and verify its identity with `gh project view` against the tracker guide.
- If none exists, create it with `gh project create --owner <configured-owner> --title "Viberaven Roadmap"`, link it with `gh project link <new-number> --owner <configured-owner> --repo <configured-repository>`, then update `docs/agents/issue-tracker.md` with its actual owner, number, and URL.
- If the create command returns an uncertain result, list Projects again before retrying so a timeout cannot create a duplicate.
- If multiple Projects match or the owner cannot be resolved from the tracker guide, ask only for that choice.
- Verify the resulting Project with `gh project view <number> --owner <configured-owner>` and `gh project field-list <number> --owner <configured-owner>`.

## Place and Update Issues

For new issues, follow `docs/agents/issue-tracker.md` and create the issue with its configured Project title so the issue and board entry are created together; verify the new issue appears in the Project.

For an existing issue, inspect `gh project item-list <number> --owner <configured-owner>` and use the configured `gh project item-add` command only when that issue is absent; verify the item after adding it.

Before changing a field, read the live fields and item values with `gh project field-list` and `gh project item-list`; use the returned field name and option spelling with `gh project item-edit`, then read the item again to verify the change.

GitHub Issues remain the source of truth for specifications and implementation tickets; Project fields carry planning status.
Read `docs/agents/triage-labels.md` only when the requested issue operation includes triage or label changes.

## Reconcile

When asked to audit or sync the board, compare current open repository issues with Project items and compare closed issue state with the Project's completion status.

Report missing board items and stale status values first; apply only the sync requested, add missing issues idempotently, and verify every changed item against live state.

Do not infer a new status from an issue title or silently close, reopen, relabel, or rewrite an issue.

## Limits

Use native `gh project` commands for supported operations and keep routine work browserless.

When the request needs views, workflows, or another feature absent from the installed `gh project` help, state the CLI limit and ask before introducing an extension or direct GraphQL mutation.

Pause only for an ambiguous owner or Project, an authorization step that requires the user, or a requested feature that needs new tooling; otherwise follow the repository configuration and complete the operation.

## Done

Report the Project and issue URLs affected, the field changes made, and the live-state check that confirmed each mutation; update `docs/agents/issue-tracker.md` whenever the canonical Project configuration changes.
