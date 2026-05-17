# Parallel Coding Agent Workflow Roadmap

## Goal

Allow multiple coding agents to develop separate features in parallel without contaminating each other's working state. Merge conflicts are acceptable and should be handled deliberately during integration; accidental cross-agent file edits, generated-file churn, and shared runtime artifacts are the problems to avoid.

## Current Assessment

This project is suitable for parallel agent development if each agent works in an isolated git worktree and branch. It is not suitable for multiple agents editing the same working directory at the same time.

Recommended baseline:

- One agent = one git worktree.
- One agent = one feature branch.
- The main worktree stays reserved for integration, review, and final validation.
- Worker agents keep changes scoped to their assigned subsystem.
- A dedicated merge agent can resolve conflicts after worker branches are ready.

Good existing properties:

- The codebase already has separable areas such as water, terrain collider, camera/gameplay, audio, UI/config, renderer/shaders, docs, and tools.
- `AGENTS.md` already documents validation expectations and generated-file cautions.
- Hidden app runs exist for end-to-end validation without visible windows.
- Logs can be queried from the binary, which is useful for headless verification.

Main risks found during the audit:

- `.gitignore` currently ignores `/resource_container_derive`, even though files under that directory are tracked. New files added there may be accidentally hidden from `git status`.
- `cargo check` regenerates tracked files under `src/app/generated/` and `src/auto-generated/`, so parallel branches can conflict on generated output.
- The runtime UI config is tracked at `config/gui.toml`, and app saves can modify it as a side effect.
- App run logs share a global temp latest-log pointer, so simultaneous hidden runs can make `--latest-log` point at another agent's run.
- `target/` is large, so multiple worktrees can consume significant disk space.
- Several large files are natural conflict hotspots: `src/app/core/mod.rs`, `src/tracer/mod.rs`, `build.rs`, `config/gui.toml`, generated Rust files, and `Cargo.lock`.

## Operating Model

### Create worker worktrees

From the integration worktree:

```bash
git worktree add ../re-flora-agent-water -b agent/water mlsmpm
git worktree add ../re-flora-agent-ui -b agent/ui mlsmpm
git worktree add ../re-flora-agent-render -b agent/render mlsmpm
```

Then run one pi session per worker directory:

```bash
cd ../re-flora-agent-water
pi
```

### Worker agent rules

Each worker agent should:

1. Start by checking `git status --short --branch`.
2. Confirm the assigned branch and task scope.
3. Avoid unrelated refactors and opportunistic cleanup.
4. Avoid editing generated files directly.
5. If shader/config changes regenerate files, include the generated output only as a consequence of `cargo check`.
6. Run the appropriate validation ladder before handing off.
7. Report changed files, validation commands, and any unvalidated behavior.

### Integration flow

In the main worktree:

```bash
git switch mlsmpm
git merge agent/water
git merge agent/ui
```

If conflicts happen, start a dedicated merge agent in the integration worktree and ask it to preserve both branches' intended behavior. Generated files should be regenerated from source instead of manually guessed.

Merge-agent prompt template:

```text
We are merging a worker branch into mlsmpm and git reports conflicts.
Inspect git status, conflict markers, and both sides' changes.
Preserve the semantic intent of both branches.
Do not discard valid logic from either side.
For generated files, resolve the source files first, then run cargo check to regenerate.
After resolving, run cargo fmt --check, cargo check, and cargo test unless blocked.
Report exactly what was resolved and what was validated.
```

## Roadmap

### Phase 0: Adopt process immediately

- Use one git worktree per pi agent.
- Keep `/home/terence/code/re-flora` as the integration worktree unless explicitly assigned otherwise.
- Keep worker tasks narrow and file scopes explicit.
- Prefer parallel coding/check/test, but serialize hidden Vulkan app runs when using `--latest-log`.
- Treat merge conflict resolution as a separate task for a merge agent.

### Phase 1: Reduce avoidable workspace friction

- Fix `.gitignore` so `resource_container_derive` itself is not ignored; ignore only its generated build artifacts if needed.
- Add or document a stable Rust toolchain for consistent worker environments.
- Consider a top-level Cargo workspace for the internal crates so metadata, lockfiles, and target layout are clearer.
- Decide whether tracked generated files remain the project policy. If they remain tracked, add a codegen/check workflow. If not, move generation to `$OUT_DIR` with `include!`.
- Split runtime config into tracked defaults plus ignored local overrides so app runs do not modify `config/gui.toml` accidentally.
- Add a per-run or per-worktree log directory option so simultaneous app runs cannot race through a shared latest-log pointer.
- Document optional `sccache` setup to reduce rebuild cost across multiple worktrees without sharing a single `target/` directory.

### Phase 2: Improve mergeability

- Continue splitting large hotspot files when making related feature changes.
- Move subsystem-specific logic out of `src/app/core/mod.rs` and `src/tracer/mod.rs` where practical.
- Keep generated-file source-of-truth comments accurate.
- Add lightweight validation commands for subsystem-only changes, so worker agents can validate without always running the full app.

### Phase 3: Optional pi orchestration

If multi-agent work becomes routine, consider a small pi extension or SDK-based helper that can:

- Create a worktree and branch for a named task.
- Write a local task brief for the worker agent.
- Launch or instruct a pi worker in that directory.
- Collect `git status`, diff summaries, validation logs, and handoff notes.
- Queue GPU/hidden app validations so they do not race on shared resources.
- Assist the integration worktree with ordered merges and conflict-resolution prompts.

This should remain optional. The clean baseline is still git worktree isolation plus normal git review/merge.
