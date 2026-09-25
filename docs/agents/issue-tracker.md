# Issue tracker: GitHub

GitHub Issues are the source of truth for specifications and implementation tickets in this repository.

Use the `gh` CLI for issue operations; commands run in this checkout resolve the repository from `origin`.

## Conventions

- Create an issue with `gh issue create --title "..." --body "..."`.
- Read an issue with `gh issue view <number> --comments` and include labels when they affect triage.
- List issues with `gh issue list --state open --json number,title,body,labels,comments` and filter to the needed status or labels.
- Comment with `gh issue comment <number> --body "..."`.
- Add or remove labels with `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
- Close an issue with `gh issue close <number> --comment "..."`.
- PRs are not a triage request surface.

## Planning project

Use the private user-owned [Viberaven Roadmap Project](https://github.com/users/izanorshahril/projects/2) to track implementation issue status.

The project is linked to `izanorshahril/viberaven`; set its default repository to this repository in GitHub project settings.

Use GitHub Issues as the durable specification; use project fields for planning status.

Link each implementation issue to the project when the issue is created.

## Wayfinding

The map is one issue labelled `wayfinder:map`; child tickets are linked as GitHub sub-issues when available.

Use native issue dependencies for blocking edges; otherwise record `Blocked by: #<number>` in the child issue body.

## Pull requests as a request surface

**PRs as a request surface: no.**

## When a skill publishes to the issue tracker

Create a GitHub issue.

When a skill fetches a ticket, run `gh issue view <number> --comments`.
