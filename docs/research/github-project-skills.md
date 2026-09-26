# GitHub Project Skills for CLI Work

Research date: 2026-09-26.

## Findings

GitHub CLI supports the core browserless workflow: `gh project create` creates a board, `gh issue create --project` can add a new issue to one, and `gh project item-add`, `item-list`, `field-create`, and `item-edit` manage existing items and fields; the project commands require the `project` scope ([create](https://cli.github.com/manual/gh_project_create), [issue create](https://cli.github.com/manual/gh_issue_create), [project commands and scope](https://cli.github.com/manual/gh_project)).

An Agent Skill supplies repeatable instructions and optional scripts; it does not replace CLI authentication, network access, or GitHub permissions ([GitHub's skill format](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/customize-cloud-agent/add-skills), [CLI scope requirements](https://cli.github.com/manual/gh_project)).

GitHub CLI also has `gh skill search`, `preview`, and `install` commands for Agent Skills; this feature is in public preview, and GitHub says third-party skills are not verified, so preview before installing ([CLI skill search](https://cli.github.com/manual/gh_skill_search), [GitHub skill guidance](https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/customize-cloud-agent/add-skills)).

The first-party skill in `cli/cli` manages skill discovery and installation, not Project boards ([`gh-skill`](https://github.com/cli/cli/blob/trunk/skills/gh-skill/SKILL.md)).

Targeted `gh skill search` queries for `gh project` and `project board` scoped to the GitHub organization returned no Project-management skill at research time.

The local environment has GitHub CLI 2.97.0, and `gh project create --help` is available, so this workspace can create Projects through the CLI without a browser.

## Community candidates

| Skill | Scope and signals | Fit for Viberaven |
| --- | --- | --- |
| [`git-project` by dmythro](https://www.skills.sh/dmythro/agent-skills/git-project) | Directly uses `gh project` and `gh api` for Project v2 setup, issue epics/sub-issues, status, priorities, and milestones; 109 installs, 7 repository stars, first seen June 16, 2026; the repo was pushed September 22, 2026 ([skill](https://github.com/dmythro/agent-skills/tree/main/git-project), [repository metadata](https://api.github.com/repos/dmythro/agent-skills)). | Closest broad match; its setup instructions include one-time UI work for views and native workflows, so it is not an all-CLI setup path. |
| [`run-github-project` by Chris Banes](https://www.skills.sh/chrisbanes/skills/run-github-project) | A live-state controller for selecting and running Project work, with mutation reconciliation and explicit `next`/`drain` modes; 411 installs, about 1.1K repository stars, first seen July 29, 2026; the repo was pushed September 25, 2026 ([skill](https://github.com/chrisbanes/skills/tree/main/skills/run-github-project), [repository metadata](https://api.github.com/repos/chrisbanes/skills)). | Strong activity and careful safeguards, but its autonomous queue-running workflow is substantially larger than Viberaven's current issue-and-status tracking. |
| [`gh-project-management` by yu-iskw](https://github.com/yu-iskw/github-project-skills) | Direct CLI operations for listing projects/items, adding issues, and updating fields; the repo had 8 stars and was pushed August 29, 2026 ([repository](https://github.com/yu-iskw/github-project-skills), [repository metadata](https://api.github.com/repos/yu-iskw/github-project-skills)). | Smaller, direct operations helper, with limited adoption evidence. |

Install counts and repository activity above were checked on the research date; `skills.sh` warns that it cannot guarantee the quality or security of listed skills ([skills.sh documentation](https://www.skills.sh/docs)).

## Round 2: Popularity and Maintenance Audit

Research date: 2026-09-26.

| Candidate | Current health signals | Relevance and limitation |
| --- | --- | --- |
| [`mattpocock/skills`](https://github.com/mattpocock/skills) | About 269K stars, 22.7K forks, 8 contributors, and an update on September 24, 2026, according to [star-history](https://www.star-history.com/mattpocock/skills/) and [Skills Docs](https://skillsdocs.com/mattpocock/skills); its GitHub releases page shows ongoing tagged releases, most recently v1.2.3 ([releases](https://github.com/mattpocock/skills/releases)). | Strongest overall planning and engineering skill suite and already the project's chosen workflow; its tracker integration targets Issues, not reusable GitHub Projects v2 board setup. |
| [`board-ops` in MCP Inspector](https://github.com/modelcontextprotocol/inspector/tree/main/.claude/skills/board-ops) | The host repo had about 10.9K stars and was pushed September 25, 2026; the contributor API returned its first 100 entries, establishing at least 100 contributors ([repo metadata](https://api.github.com/repos/modelcontextprotocol/inspector), [contributors](https://api.github.com/repos/modelcontextprotocol/inspector/contributors?per_page=100)). | Strongest direct project-board skill by host-repository health, with evals, but it is tied to MCP Inspector's own project IDs, fields, and operating rules, so copying it would carry the wrong board configuration. |
| [`langwatch-kanban`](https://github.com/langwatch/langwatch/blob/main/.claude/skills/langwatch-kanban/SKILL.md) | The host repo has about 3.3K stars, 5,000+ commits, and visible continuing PR activity ([repository](https://github.com/langwatch/langwatch), [PR activity](https://github.com/langwatch/langwatch/pulls)). | Direct `gh` and GraphQL board operations, but the skill hard-codes LangWatch's org, repository, project number, field IDs, and a developer-specific Claude memory path; not portable as-is. |
| [`github/gh-aw`](https://github.com/github/gh-aw) | GitHub's maintained CLI extension has about 5.2K stars, more than 500 forks, and active documentation ([repository](https://github.com/github/gh-aw), [CLI reference](https://github.github.com/gh-aw/setup/cli/)). | Not an Agent Skill, but `gh aw project new` can create user or org boards, link a repository, and optionally create standard views and fields; it adds a separate extension when native `gh project` already covers basic board creation and issue/field operations. |

The repo star count measures interest in the host project, not use or quality of its individual skill; board-specific candidates inspected here were tailored to their host repos, while the broadly popular Matt Pocock suite does not provide Project v2 board management.

## Updated Recommendation

Use the existing Matt Pocock skills for specs, ticket breakdown, and implementation, and use the native `gh project` commands for board creation, issue placement, and field updates.

Use `gh-aw` only if the project requires its standard board setup of views and fields; do not add a third-party project skill whose board IDs and rules belong to another repository.

The resulting Viberaven-local skill is [.agents/skills/viberaven-project-board/SKILL.md](../../.agents/skills/viberaven-project-board/SKILL.md); it combines CLI-first setup, live-state verification, idempotent issue placement, and board hygiene while reading this repository's tracker configuration.

No external skill was installed; the single repo-local skill reuses the researched patterns without importing another project's board IDs or policies.
