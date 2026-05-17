# Parallel Agent Preparation Roadmap

## Purpose

This roadmap lists the preparation work needed before parallel coding-agent development becomes a regular workflow for this project.

The goal is simple: multiple agents should be able to develop different features at the same time without corrupting each other's working state. Merge conflicts may still happen during integration, but day-to-day development should stay isolated, auditable, and easy to reset.

## Target Workflow

- One coding agent works in one git worktree.
- One worktree uses one feature branch.
- The main project worktree is reserved for integration, review, and final validation.
- Worker agents keep their changes scoped to the assigned task.
- A separate merge-agent pass resolves conflicts when worker branches are merged back.

Example layout:

```text
/home/terence/code/re-flora              # integration worktree
/home/terence/code/re-flora-agent-water  # worker: agent/water
/home/terence/code/re-flora-agent-ui     # worker: agent/ui
/home/terence/code/re-flora-agent-render # worker: agent/render
```

## Step 1: Establish the Worktree Convention

### Actions

- Use git worktrees for every parallel agent task.
- Name worker branches with a clear prefix, for example `agent/water`, `agent/ui`, or `agent/render`.
- Keep the existing main worktree as the integration worktree unless a task explicitly says otherwise.
- Document the standard setup command:

```bash
git worktree add ../re-flora-agent-water -b agent/water mlsmpm
cd ../re-flora-agent-water
pi
```

### Done Criteria

- Every worker agent starts in its own directory.
- No two editing agents share the same working directory.
- The integration worktree stays clean except during review and merge work.

## Step 2: Define Worker-Agent Rules

### Actions

- Require every worker agent to start with:

```bash
git status --short --branch
```

- Require each worker to confirm its task scope before editing.
- Keep worker changes narrow and related to one feature or subsystem.
- Avoid unrelated cleanup, formatting churn, and opportunistic refactors.
- Require worker handoff notes to include:
  - changed files
  - validation commands run
  - known risks or unverified behavior
  - whether generated files changed

### Done Criteria

- Worker output is reviewable as a focused branch.
- Handoff notes are enough for the integration agent to merge or reject the work.

## Step 3: Clean Up Ignore Rules

### Current Issue

`.gitignore` currently ignores `/resource_container_derive`, but files under that directory are already tracked. This can hide newly added files in that crate from `git status`.

### Actions

- Stop ignoring the whole `resource_container_derive` directory.
- Ignore only generated artifacts if needed, such as:

```gitignore
/resource_container_derive/target/
/resource_container_derive/Cargo.lock
```

### Done Criteria

- New source files added under `resource_container_derive/` appear in `git status`.
- Build artifacts remain ignored.

## Step 4: Make the Toolchain Reproducible

### Actions

- Add or document a stable Rust toolchain for all agents.
- Confirm Linux and macOS bootstrap instructions list the required Vulkan, shader, CMake, compiler, and audio dependencies.
- Document that agents should source the required shell environment before full app validation:

```bash
source ~/.zshrc
```

### Done Criteria

- A new worker worktree can run `cargo check` without local guesswork.
- Environment setup differences are documented instead of rediscovered by each agent.

## Step 5: Decide the Generated-File Policy

### Current Issue

`cargo check` regenerates tracked files:

```text
src/app/generated/gui_adjustables_gen.rs
src/auto-generated/gpu_structs.rs
```

This is workable, but it can create merge conflicts when multiple agents change shader or GUI config sources.

### Decision

Use option 1 for now: keep generated files tracked.

### Actions

- Keep generated files tracked in git for the current workflow.
- Document that agents must not hand-edit them.
- Resolve shader or GUI config sources first, then run `cargo check` to regenerate generated output.
- Include generated diffs only when they are a consequence of source changes.
- Keep the `OUT_DIR` approach as a possible future cleanup, but do not do it during the initial parallel-agent preparation.

### Done Criteria

- Agents know whether generated diffs are expected.
- Merge agents know not to manually guess generated-file conflict resolutions.

## Step 6: Isolate Runtime Configuration

### Current Issue

The app can save runtime UI settings directly into tracked `config/gui.toml`. A validation run can therefore create unrelated config diffs.

### Decision

Do not implement this step yet. Runtime config isolation is recorded as future work.

### Actions

For now:

- Keep the current `config/gui.toml` behavior unchanged.
- Treat changes to `config/gui.toml` after app runs as suspicious unless the task intentionally changes GUI defaults.
- Worker agents should check whether `config/gui.toml` became dirty before handoff and report whether the change is intentional.

Future cleanup:

- Split tracked defaults from local mutable configuration.
- Suggested layout:

```text
config/gui.default.toml  # tracked source of defaults
config/gui.local.toml    # ignored local override
```

- Make app saves write to the local override, not the tracked default.

### Done Criteria

- Running the app does not modify tracked config unless the task intentionally changes defaults.
- Worker agents can validate without producing unrelated config churn.

## Step 7: Isolate App Run Logs

### Current Issue

The latest-run-log pointer is shared through a temp directory. If multiple agents run hidden app validation at the same time, `--latest-log` may point to another agent's run.

### Actions

- Add a per-run, per-worktree, or environment-configurable log directory.
- Until then, serialize hidden Vulkan app runs when logs are part of validation.
- Tell agents to capture the exact log path printed by the run instead of blindly trusting `--latest-log` during parallel validation.

### Done Criteria

- A worker can identify its own app log reliably.
- Integration validation is not confused by another agent's run.

## Step 8: Plan Build Cache and Disk Usage

### Current Issue

The top-level `target/` directory can become very large. Multiple worktrees multiply disk usage.

### Actions

- Prefer separate `target/` directories for isolation.
- Use `sccache` to share compilation cache safely across worktrees.
- Avoid sharing one `CARGO_TARGET_DIR` by default, because it can make parallel builds noisier and less isolated.
- Periodically remove old worker worktrees and their `target/` directories.

### Done Criteria

- Parallel workers do not block each other through a shared target directory.
- Disk usage remains predictable.

## Step 9: Identify Conflict Hotspots

### Current Hotspots

The following files are likely to conflict during parallel work:

```text
src/app/core/mod.rs
src/tracer/mod.rs
build.rs
config/gui.toml
src/app/generated/gui_adjustables_gen.rs
src/auto-generated/gpu_structs.rs
Cargo.lock
```

### Actions

- Assign tasks so workers avoid editing the same hotspot files when possible.
- When modifying a hotspot file, keep the change minimal and well explained.
- Prefer moving subsystem-specific logic into smaller modules during related feature work.

### Done Criteria

- Parallel tasks are scoped to reduce avoidable conflicts.
- Necessary conflicts are localized and understandable.

## Step 10: Define the Merge-Agent Procedure

### Actions

Use a dedicated merge-agent pass when conflicts occur. Suggested prompt:

```text
We are merging a worker branch into mlsmpm and git reports conflicts.
Inspect git status, conflict markers, and both sides' changes.
Preserve the semantic intent of both branches.
Do not discard valid logic from either side.
For generated files, resolve the source files first, then run cargo check to regenerate.
After resolving, run cargo fmt --check, cargo check, and cargo test unless blocked.
Report exactly what was resolved and what was validated.
```

### Done Criteria

- Merge conflict resolution is explicit, reviewable, and validated.
- Generated-file conflicts are regenerated from source instead of manually edited.

## Step 11: Run a Small Pilot

### Actions

- Start with two worker agents on clearly separated tasks.
- Merge both branches back through the integration worktree.
- Record what caused friction:
  - unexpected dirty files
  - generated-file churn
  - validation failures
  - log confusion
  - disk usage
  - merge conflicts

### Done Criteria

- The team has one successful end-to-end parallel-agent cycle.
- The process can be adjusted before scaling to more agents.

## Step 12: Optional Pi Orchestration Later

This is not required for the initial workflow. If parallel work becomes frequent, consider a small pi extension or SDK-based helper that can:

- create a worktree and branch for a task
- write a local task brief
- launch or guide a pi worker in that directory
- collect status, diffs, validation logs, and handoff notes
- queue hidden app validations to avoid shared-resource races
- assist with ordered integration merges

The baseline should remain simple: git worktree isolation, focused worker branches, and deliberate integration in the main worktree.
