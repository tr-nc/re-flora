# Issue tracker: GitHub

Issues and PRDs for this repository live in GitHub Issues. Use the `gh` CLI from this checkout so the repository is inferred from `git remote -v`.

## Conventions

- Create an issue with `gh issue create`.
- Read an issue and its discussion with `gh issue view <number> --comments`.
- Apply or remove labels with `gh issue edit <number> --add-label <label>` and `--remove-label <label>`.
- Comment with `gh issue comment <number>` and close with `gh issue close <number>`.

## Pull requests as a triage surface

PRs as a request surface: no.

## Skill terminology

When an engineering skill says to publish to the issue tracker, create a GitHub issue in this repository. When it says to fetch the relevant ticket, read the matching GitHub issue, including comments and labels.
