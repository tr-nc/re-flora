# Claude Command: Commit

This command helps you create well-formatted commits with conventional commit messages.

## Usage

To create a commit, just type:

```shell
/commit
```

## What This Command Does

1. Checks which files are staged with `git status`
2. If 0 files are staged, warn the user that no files are staged and ask if they want to stage all files
3. If the user wants to stage all files, automatically adds all modified and new files with `git add`
4. Performs a `git diff` to understand what changes are being committed
5. Analyzes the diff to determine if multiple distinct logical changes are present
6. If multiple distinct changes are detected, suggests breaking the commit into multiple smaller commits
7. For each commit (or the single commit if not split), creates a commit message using emoji conventional commit format

## Best Practices for Commits

- **Verify before committing**: Ensure code is linted, builds correctly, and documentation is updated
- **Atomic commits**: Each commit should contain related changes that serve a single purpose
- **Split large changes**: If changes touch multiple concerns, split them into separate commits
- **Conventional commit format**: Use the format `<type>: <description>` where type is one of:
  - `feat`: A new feature
  - `fix`: A bug fix
  - `docs`: Documentation changes
  - `style`: Code style changes (formatting, etc)
  - `refactor`: Code changes that neither fix bugs nor add features
  - `perf`: Performance improvements
  - `test`: Adding or fixing tests
  - `chore`: Changes to the build process, tools, etc.
- **Present tense, imperative mood**: Write commit messages as commands (e.g., "add feature" not "added feature")
- **Concise first line**: Keep the first line under 72 characters

## Guidelines for Splitting Commits

When analyzing the diff, consider splitting commits based on these criteria:

1. **Different concerns**: Changes to unrelated parts of the codebase
2. **Different types of changes**: Mixing features, fixes, refactoring, etc.
3. **File patterns**: Changes to different types of files (e.g., source code vs documentation)
4. **Logical grouping**: Changes that would be easier to understand or review separately
5. **Size**: Very large changes that would be clearer if broken down

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

Example of splitting commits:

- First commit: feat: add new solc version type definitions
- Second commit: docs: update documentation for new solc versions
- Third commit: chore: update package.json dependencies
- Fourth commit: feat: add type definitions for new API endpoints
- Fifth commit: feat: improve concurrency handling in worker threads
- Sixth commit: fix: resolve linting issues in new code
- Seventh commit: test: add unit tests for new solc version features
- Eighth commit: fix: update dependencies with security vulnerabilities
