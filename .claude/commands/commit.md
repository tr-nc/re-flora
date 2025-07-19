# Claude Command: Commit

This command helps you create well-formatted commits with conventional commit messages, only the staged files will be committed.

## Usage

To create a commit, just type:

```shell
/commit
```

## Description

1. Minimal effort commit, only the staged files will be committed.
2. Claude will not use git log to see any previous commits, and will only follow the conventional commit messages as demonstrated in the examples. This balances speed, consistency, and accuracy. Claude will not write any commit description for you for simplicity and speed.
3. When finished, Claude will say: Commit done! [commit-message].

## Examples

Good commit messages:

- feat: add user authentication system
- fix: resolve memory leak in rendering process
- docs: update API documentation with new endpoints
- refactor: simplify error handling logic in parser
- fix: resolve linter warnings in component files
- chore: improve developer tooling setup process
- feat: implement business logic for transaction validation
- fix: address minor styling inconsistency in header
- fix: patch critical security vulnerability in auth flow
- style: reorganize component structure for better readability
- fix: remove deprecated legacy code
- feat: add input validation for user registration form
- fix: resolve failing CI pipeline tests
- feat: implement analytics tracking for user engagement
- fix: strengthen authentication password requirements
- feat: improve form accessibility for screen readers
