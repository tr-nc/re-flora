# Agent workflow: player-facing release packages

Use this workflow whenever the user asks to package, build the official three-platform
downloads, release, publish a version, or trigger `itch-builds.yml`. A package request
includes writing its player-facing update notes; do not ask the user to write them.
Preparing release tooling is not itself permission to publish a game version.

## 1. Establish what is actually being shipped

- Work from the main worktree on clean, up-to-date `main`, as required by AGENTS.md.
- Inspect the latest published GitHub Release and resolve its tag/commit. Compare that
  released commit with the intended target using `git log` and `git diff`, plus relevant
  gameplay docs and completed validation. Use the last shipped release, not merely the
  most recent local tag or a previous PR, as the player comparison baseline.
- Confirm whether this is a public version tag or a manual preview package. Manual
  packages also need a specific version, e.g. `0.4.1-preview.1`, and their own notes.
- Do not include unmerged work, roadmap promises, or unverified platform claims. A
  hidden smoke run does not prove manual play, audio quality, or performance.

## 2. The agent writes the update

Write `docs/releases/<exact-package-version>.md`. Use English for the public notes,
matching the player-facing README; give the user a concise Chinese summary if helpful.
The document starts with `# Re: Flora <exact-package-version>` and contains these
three section headings:

- `## What's new`: a short opening summary followed by 2–5 concrete player-visible
  changes where available. Explain what the player can do, see, hear, or experience.
  Prefer “Fallen fruit settles more steadily on the ground” to shader/module names,
  test totals, commit dumps, or “various improvements.” For a packaging-only update,
  accurately say gameplay is unchanged and explain the packaging fix.
- `## Known limitations`: current prototype limitations and relevant known issues.
  State the actual scope; do not invent assurances that all platforms were played.
- `## How to play`: official download link, platform package choice, extract/run
  instruction, and the version's playing guide. Keep the current Vulkan/driver
  requirements accurate. Include a feedback link.

You may add a comparison/source link or an actual version-matched screenshot/video.
Do not use TODO/TBD placeholders or fake changelog entries. The tooling verifies
presence and structure, not factual accuracy: the agent owns the semantic review.

Commit the notes before tagging or dispatching, then run:

```sh
python3 scripts/release_notes.py --version 0.4.1
python3 scripts/release_tag.py --bump-patch --dry-run
```

Use the actual intended version. Missing, uncommitted, wrong-version, empty-section,
or placeholder notes fail before a tag is created. The notes are read from `HEAD`,
so a local uncommitted file cannot accidentally describe a different shipped commit.

## 3. Trigger and verify

For a user-authorized public patch/minor release, use the existing release helper
with `--bump-patch -y` or `--bump-minor -y` after the dry run. It bumps version files,
commits, pushes main and the tag. Do not bump or tag merely to validate this workflow.

For a user-authorized manual package, commit the exact version's notes and dispatch:

```sh
gh workflow run itch-builds.yml --ref main -f version=0.4.1-preview.1
```

Manual main-branch runs build artifacts only; the publish job requires a tag ref and
successful platform jobs. Manual inputs are not a bypass for player notes.

CI validates notes before starting the three builds. Each zip contains exactly the
committed document as root `RELEASE_NOTES.md`; the workflow summary also shows it.
The GitHub Release is created with the same document using `--notes-file`. Re-running
an existing release uploads packages but preserves its existing body (including later
editorial corrections); it never replaces it with generic automated text. Correct
an existing body intentionally with `gh release edit --notes-file` if needed.

After triggering, identify and inspect the corresponding run and all three jobs.
When complete, verify three assets, the Release body (for tag runs), and the notes
inside each zip. Report pending or failed builds honestly. Link the notes and run
in the handoff so the user can see what was shipped without reading Git history.

## Release notes are not the development log

Keep engineering evidence in commits/PRs and development docs. Keep these notes
short, readable, version-specific, and grounded in the actual shipped behavior.
