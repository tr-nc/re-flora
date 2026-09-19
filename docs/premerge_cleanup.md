# DDGI / tree pre-merge cleanup

## Scope and decisions

The user accepted the digging-performance improvement and requested direct pre-merge cleanup.
Only the reviewed DDGI/tree feature is selected for integration into `main`; unrelated butterfly
and terrain-material work is not selected. No release, remote-branch deletion, forced deletion,
reset, or additional agent is authorized or performed as part of this cleanup.

- Preserve both saved DDGI A/B controls, their old IDs, migration, and **false** defaults. They
  remain experimental rather than being silently promoted or removed from old saved settings.
- Move them out of general Lighting Diagnostics into a separate, initially collapsed
  **Debug → DDGI Experiments** group. Explicitly state that they are not needed for the digging
  performance fix and preserve the 64-spacing regression warning.
- Keep the validated native digging regression runner and tests. Do not delete useful diagnostics
  merely to make the branch look smaller.
- Preserve both checkpoint tags and historical reports. Black patches remain **not reproduced
  again by the user after the stall fix**, not a proven fixed root cause.
- Archive ignored worker evidence before removing completed, contained DDGI worktrees.

## Complete initial review ledger

Inventory covered every local branch and registered checkout, including untracked-file status.
There were no detached checkouts or local branches without checkouts. `origin/main` was fetched
and matched local `main` at `d7bbee241c6880df00d71300bffa77eef1262bce`. Relationships below are
against that frozen main; the approved feature tip already contains main. Complete raw status,
unique commit lists and diff statistics are in `target/premerge-cleanup/inventory.json` in the
feature checkout. Herdr workspace/agent inventories were inspected without controlling other agents.

| Item / branch | Path | Frozen HEAD | Cleanliness | Target relation / value | Worker state | Action / blocker |
|---|---|---|---|---|---|---|
| `main` | `/home/terence/code/re-flora` | `d7bbee241c6880df00d71300bffa77eef1262bce` | clean | target | w4, idle | preserve; advance only after staged validation |
| `agent/butterfly-block-flight` | `/home/terence/code/re-flora-agent-butterfly-block-flight` | `7dc2b144b318202864b6346833f9024454709111` | clean | 12 ahead; approved feature, valuable | wX, current active session | integrate after cleanup; retain active checkout |
| `agent/ddgi-history-candidate` | `/home/terence/code/re-flora-agent-ddgi-history-candidate` | `0b3d006f93608ababcb3d5c8669b48ef19f6dc8f` | clean | 10 ahead; exact tip contained in approved feature | completed earlier, no Herdr workspace | archive evidence, remove only after target contains tip |
| `agent/ddgi-history-reuse` | `/home/terence/code/re-flora-agent-ddgi-history-reuse` | `06d8257ba7fa02ac97ef2c1dd4cbb21fcfacdf95` | clean | 7 ahead; exact tip contained in approved feature | completed earlier, no Herdr workspace | archive evidence, remove only after target contains tip |
| `research/ddgi-edit-history` | `/home/terence/code/re-flora-research-ddgi-history` | `c045d1b058cac7e1ca809ec26f34e3f412dcb530` | clean | main 3 / source 1; exact tip contained in approved feature | completed earlier, no Herdr workspace | remove only after target contains tip; no ignored evidence |
| `butterfly-blend` | `/home/terence/.herdr/worktrees/re-flora/butterfly-blend` | `4701ff5332c4510d356de6df296949639cb4e156` | dirty (GUI, shaders, renderer, validation changes) | 19 ahead; separate active butterfly implementation, not accepted here | w19, working | retain; active, dirty, outside scope |
| `agent/terrain-material-refresh` | `/home/terence/code/re-flora-agent-terrain-material-refresh` | `7bdd3640f9311f49152dbf5c543088e45bc1a02b` | clean | 2 ahead; procedural material candidate awaiting separate acceptance | w1A, idle | retain; uncontained, outside scope |
| `codex/terrain-material-candidate` | `/home/terence/code/re-flora-terrain-material` | `c47050223d0394f2e36eb3074b8caa646ad5fe7a` | clean | main 201 / source 1; WIP material candidate, contained in material-refresh branch but not main | no Herdr workspace | retain; uncontained, outside scope |
| `agent/butterfly-pipeline-research` | `/tmp/re-flora-butterfly-pipeline-research` | `4ec166d452566d77fb3e11d1754b5b2226d81fe3` | clean | 2 ahead; research/prototype ancestry contained in active butterfly work, not main | no Herdr workspace | retain; uncontained, belongs to separate feature |
| `agent/butterfly-runtime-research` | `/tmp/re-flora-butterfly-runtime-research` | `215760dfec4502b5a5270cd4f83200d25cad120b` | clean | 8 ahead; research/prototype ancestry contained in active butterfly work, not main | no Herdr workspace | retain; uncontained, belongs to separate feature |

Containment graph: approved feature → DDGI candidate → DDGI measurement; approved feature also
contains DDGI research. Butterfly feature → both butterfly research tips. Material-refresh →
material candidate. No redundant merges are needed. Unrelated work is preserved, not declared
obsolete or safe to integrate merely because its checkout is clean.

## Evidence archive

Stable archive root: `/home/terence/code/re-flora/target/collected-worktree-evidence/`.
Archives retain ignored files except regenerable Rust `target/debug`, `target/release`, and
`target/tmp` build directories. Tracked source/reports remain in Git history and remote refs.
These archives are local evidence, not committed binary payloads or off-machine backups.

| Archive | SHA-256 |
|---|---|
| `re-flora-agent-ddgi-history-candidate.tar.zst` | `8d69c4486f26103bfb0844c17459d5811aa231614e238f3dc1d151bd56b2bc12` |
| `re-flora-agent-ddgi-history-reuse.tar.zst` | `78beec62937f69f03bd07bd4c94a5b42613ead1ffa2de5a3c432d9ba435dc08b` |

Both archives passed `zstd -t`; `manifest.json` records their original paths and exact revisions.
Historical report paths remain provenance. To inspect an old result, extract the appropriate
archive into a fresh directory; paths inside preserve the original worktree-relative layout.

## Completion record

Integrated source `3b69cce8b8285b7fb07eb4af9efe15fd771a2f6b` into `main` by a validated
fast-forward, then pushed and verified `origin/main` equality. This completion record and the
archive pointers are a documentation-only follow-up; no rendering policy was changed afterwards.

Removed, after rechecking exact tips, cleanliness, absence of a Herdr workspace, and containment
in main: the DDGI candidate, DDGI measurement, and DDGI research worktrees and their three local
branches. Used non-force `git worktree remove` / `git branch -d`. Remote branches were not deleted.
The operation-owned `/tmp/re-flora-ddgi-premerge-integration` checkout and
`integration/ddgi-premerge-cleanup` branch were removed last; its run logs were preserved.

Final inventory: main and the current feature checkout are retained at the integrated source plus
this documentation record. The other retained checkouts/branches are exactly those in the initial
ledger: butterfly-blend (advanced to `ed309bc7efcfb30aed3dea8fca2d551cee4c83d0`, with uncommitted
saved GUI values), terrain-material-refresh (`7bdd3640`), terrain-material-candidate (`c4705022`),
butterfly-pipeline-research (`4ec166d4`), and butterfly-runtime-research (`215760df`). The latter
four revisions did not move. None was merged or removed as part of this scoped cleanup.

Validation: fmt/check; **1042 main + 4 library tests passed, 2 ignored**; **73 targeted Python**
and **17 Slang CPU** tests passed. The isolated staged release passed default hidden/muted startup,
tree/resize/wood-edit smoke, and the native terrain-edit regression. Main was rebuilt and independently
passed the native regression (zero during-edit tree recompiles, maximum logged edit frame **19.02 ms**)
and hidden/muted release startup with canonical log inspection. Main check/tests were repeated.
Saved GUI/camera bytes were restored; no generated source changed.

An initial attempt to reuse main's Cargo target directory across the temporary checkout reused old
shader artifacts and failed at the irradiance-filter descriptor binding. That failed attempt was
not accepted: an isolated staging build compiled all 117 shaders and passed. Main's stale per-package
Vulkan build cache was then cleaned in both release and dev profiles and regenerated in main; the
fresh main release passed. No renderer workaround was added, and cross-worktree Cargo target reuse
should be avoided. Failed logs and successful reruns are retained under
`target/premerge-cleanup/` in the feature checkout, with a copy alongside the evidence archives.

**Review complete:** every initial worktree/local branch was inventoried and classified.
**Scoped cleanup complete:** all three completed DDGI worker checkouts were safely removed.
**Repository-wide collection incomplete, intentionally:** the active caller checkout and separate
butterfly/material work remain; their deletion or wholesale integration was not part of this cleanup.

### Subsequent user instructions

After this cleanup, the user explicitly requested main-to-Butterfly-Blend integration, deletion of
our temporary checkpoint tags, pushing main, and shutdown after integration. Those are separate
follow-up operations. Butterfly Blend is settled but retains user GUI changes (tree mode/wind,
butterfly resolution/FPS/transmission and preview). Preserve those changes; do not silently commit,
stash, discard, or overwrite them to make the checkout clean.

Both temporary tags listed above were subsequently deleted locally and from origin with explicit
user authorization; remote absence was verified. Their underlying commits remain ancestors of main.
The tag references earlier in this document describe the historical preservation step, not live refs.
