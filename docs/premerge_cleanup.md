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

Pending final staged validation, target advancement, and containment-checked removal. The
initial inventory is preserved above; the final action record will be appended after completion.
This is scoped feature cleanup, not authorization to collect the entire repository into main.
