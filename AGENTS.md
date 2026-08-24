# AGENTS.md

## Development Notes

- Keep changes small and focused.
- Prefer root-cause-correct solutions with clean, extensible designs over short-term tricks. Do not ship a workaround merely because it is faster; use tricks only as explicitly labeled diagnostics or temporary experiments when requested.
- Commit each validated step before starting the next one.
- After completing and validating a requested feature or fix, commit it automatically; do not wait for the user to ask for a commit.
- Prefer measuring before guessing on performance work.
- For performance work, release-mode app benchmarks are authoritative; debug builds and unit tests are not performance evidence.
- Run `cargo check` after shader or Rust changes. It also regenerates shader-derived Rust structs.
- Validate Rust/rendering changes with hidden muted mode (`cargo run --release -- --hidden --mute --auto-exit 0.5`) and inspect the run log for errors.
- Do not edit generated files directly unless they are part of the generated output from a build/check.

## Release Versioning

Use the main worktree (`/home/terence/code/re-flora`) on a clean, up-to-date `main` branch for releases. Do not release from worker worktrees.

- First check and update: `git status --short --branch` then `git pull --ff-only`.
- Patch release: run `scripts/release_tag.py --bump-patch -y`. The helper bumps `Cargo.toml` and `Cargo.lock`, commits `bump version to X.Y.Z`, pushes `main`, creates annotated tag `vX.Y.Z`, and pushes the tag to trigger `.github/workflows/itch-builds.yml`.
- Minor release: run `scripts/release_tag.py --bump-minor -y`. The helper computes `X.(Y+1).0`, updates only the root `re-flora` version in `Cargo.toml` and its matching `Cargo.lock` package block, commits `bump version to X.(Y+1).0`, pushes `main`, creates annotated tag `vX.(Y+1).0`, and pushes the tag to trigger `.github/workflows/itch-builds.yml`.
- After triggering a release, confirm CI with `gh run list --workflow itch-builds.yml --limit 3`; use `gh run watch <run-id>` if the user asks to wait for packages.

## Parallel Agent Workflow

Use git worktrees to keep parallel coding agents isolated. Do not run multiple agents that edit files in the same working directory.

- One agent = one git worktree = one feature branch.
- Keep the main project worktree for integration, review, and final validation unless explicitly assigned otherwise.
- Create worker branches with names like `agent/water`, `agent/ui`, or `agent/render`.
- Keep each worker task narrow and identify likely file boundaries before editing.
- Avoid unrelated cleanup in worker branches; it increases merge conflict risk.
- Generated files remain tracked for now. Do not hand-edit them; resolve the shader/config source first, regenerate with `cargo check`, and include generated diffs only when they follow from source changes.
- Runtime GUI config isolation is not implemented yet. Treat `config/gui.toml` diffs after app runs as suspicious unless the task intentionally changes defaults.
- Run logs are stored per worktree under `target/re-flora-logs`; use `--latest-log` and `--tail-latest-log` from the same worker worktree that produced the run.

### Worktree Convention

Use the existing `/home/terence/code/re-flora` checkout as the integration worktree by default. Start each parallel worker from a sibling worktree and a dedicated branch:

```bash
git worktree add ../re-flora-agent-water -b agent/water mlsmpm
cd ../re-flora-agent-water
pi
```

Use clear branch and directory names that describe the task or subsystem, such as `agent/water`, `agent/ui`, or `agent/render`. Before starting or merging work, confirm active worktrees with:

```bash
git worktree list
```

### Worker Agent Checklist

At the start of every worker task, run:

```bash
git status --short --branch
```

Then confirm the current branch, assigned scope, and likely file boundaries before editing. Keep worker branches focused on the assigned feature or subsystem, and avoid unrelated cleanup, formatting churn, or opportunistic refactors.

Worker handoff should include:

- changed files
- validation commands run
- known risks or unverified behavior
- whether generated files changed

Integration happens in the main worktree with normal git merges. If conflicts occur, use a dedicated merge-agent pass that inspects both sides, preserves both intended behaviors, and regenerates generated files from their sources rather than guessing.

## Validation Policy

Keep `cargo test` fast and deterministic. Unit tests are valuable for pure logic guardrails such as chunk math, queue behavior, allocator correctness, CPU voxel sampling, and revision tracking. Do not turn long-running random benchmarks, GPU/window/audio checks, or perf experiments into normal unit tests; make them lightweight, `#[ignore]`, bench targets, scripts, or hidden app runs instead.

Use real app runs plus logs for end-to-end verification, especially for Vulkan, audio, windowing, terrain editing, water collider refreshes, and performance regressions. A good default validation ladder is:

```bash
cargo fmt --check
cargo check
cargo test
cargo run --release -- --hidden --mute --auto-exit 0.5
```

Inspect the generated per-worktree run log after hidden runs. Prefer checking concrete expected log lines, hashes, dimensions, timings, and absence of errors over relying on visual behavior when running headless. Use the built-in log helpers from the same worktree instead of guessing file names:

```bash
cargo run --release -- --latest-log
cargo run --release -- --tail-latest-log 200
```

### User Try-Out Role

After implementing and validating a change, do not automatically launch the visible game. If the user says they want to try it, visualize it, experience it, or otherwise asks for a live/manual check, run the game in visible mode with plain `cargo run` and no `--hidden` flag. If the implementation lives in a worker worktree, run `cargo run` from that same worktree so the user tests the intended branch and assets.

## Basic Perf Test

Benchmark in release mode is king. Use `cargo run --release` hidden app runs with logs for performance decisions.

Discover CLI usage and testing recipes from the binary:

```bash
cargo run --release -- --help
cargo run --release -- -h
```

Prefer `--hidden --mute` for background validation; `--hidden` keeps the normal native window, Vulkan surface, and swapchain path, but leaves the window invisible, while `--mute` silences global audio output. Do not pass `--present-mode` by default; let the app auto-select it.

## Flora Instance Perf Note

The 4-byte flora instance vertex stride was slower than the 8-byte stride on the tested GPU/driver. Keep the aligned 8-byte instance vertex layout unless a replacement path, such as storage-buffer instance fetch, benchmarks better.

## Agent skills

### Issue tracker

Issues and PRDs are tracked in this repository's GitHub Issues. See `docs/agents/issue-tracker.md`.

### Triage labels

Engineering skills use the five canonical triage labels documented in `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repository with its glossary in `CONTEXT.md` and architectural decisions under `docs/adr/` when present. See `docs/agents/domain.md`.
