# Playing Re: Flora

This guide explains how to download, launch, and explore the current Re: Flora prototype. The latest
release is an evolving systems playground; the complete care, harvest, and Seed Circulation loop in
[First Garden Moment](https://github.com/tr-nc/re-flora/blob/main/docs/first_garden_moment.md) is still
a longer-term product milestone. This source branch also contains an independent, unsaved
[Rooftop PoC](rooftop_poc.md), launched with `cargo run --release -- --rooftop-poc`; it is not a claim
that an existing packaged release includes this experiment.

This copy of the guide tracks the current branch. Every packaged release includes the matching guide
from its own source revision. Because development continues between tags, follow the guide inside the
downloaded archive if its controls differ from this page.

## Download

Open **[the latest release](https://github.com/tr-nc/re-flora/releases/latest)** and choose the zip
whose filename ends with your platform:

- `windows.zip`
- `macos.zip`
- `fedora.zip`

The link always resolves to the newest non-prerelease GitHub release; this guide does not depend on a
specific version number.

## Requirements

- A Vulkan-capable GPU with a current graphics driver. RTX hardware is not required.
- Windows, macOS, or Fedora matching one of the packaged downloads.
- Enough free space to extract the complete archive.

Packaged releases include the non-system runtime libraries used by the game. You do not need Rust,
Slang, the Vulkan SDK, or a source checkout to play. The macOS package includes MoltenVK.

## Launch

Keep the extracted executable, `assets`, `config`, and bundled library folders together.

### Windows

1. Extract the complete `windows.zip` archive.
2. Open the extracted folder.
3. Run `re-flora.exe`.

If Windows shows a reputation warning for the unsigned prototype, review the publisher and source
before choosing to run it.

### macOS

1. Extract the complete `macos.zip` archive.
2. Open the extracted folder.
3. Run `re-flora`.

The prototype is not distributed as a notarized App Store application. If macOS blocks the first
launch, use **System Settings -> Privacy & Security -> Open Anyway** after verifying that the archive
came from the official Re: Flora release page.

### Fedora

1. Extract the complete `fedora.zip` archive.
2. Open a terminal in the extracted folder.
3. Run `./re-flora`.

The Linux package targets the Fedora version used by the release workflow. Other distributions are
not currently advertised as packaged targets.

### Flat terrain for debugging

Launch with `re-flora --flat-terrain` (from source: `cargo run --release -- --flat-terrain`)
to start with level soil at world height 0.5. Normal soil/rock layers and terrain editing remain
available. This startup option is not saved as a preference; omit it for normal terrain.
It cannot be combined with `--terrain-load` or fixed scene/diagnostic flags. You can still save
the generated terrain with `--terrain-save` or load another world later in the Debug panel.

## A First Visit

The current build has prototype controls rather than a finished tutorial. This short sequence shows
the most relevant garden interactions:

1. Move through the world and find an area of exposed soil.
2. Press **1** for Grow. In the right panel choose **Type → Flora**, then a variety, and hold the
   left mouse button over soil to plant. Choose **Type → Tree** and click once to plant a tree.
   **Tab** cycles varieties within the selected Type.
3. Press **6** for Water and hold the left mouse button over the planted area. Watch the soil and
   growth response.
4. Press **7** for the soil inspector and hover over terrain to inspect moisture.
5. Try **3** to dig, **4** to smooth, **5** to trim, or **8** to till.
6. Press **X** to place a sprinkler.
7. Press **G** when you want to switch between editing the garden and walking through it.

The current prototype does not yet provide the complete harvest, Seed Circulation, automatic return,
or player onboarding described by the canonical game direction.

## Controls

### Camera and Application

| Input | Action |
| --- | --- |
| **WASD** or **arrow keys** | Move or pan, depending on camera mode |
| **Q / E** | Lower / raise the orbit editing camera |
| **Middle mouse drag** | Pan the orbit camera |
| **Right mouse drag** | Rotate the orbit camera |
| **Mouse wheel** | Zoom the orbit camera or adjust the active edit radius |
| **G** | Switch between orbit editing and free-look camera modes |
| **Left Shift** | Move faster in free-look mode |
| **F** | Toggle fullscreen |
| **M** | Toggle mute |
| **P** | Copy a screenshot to the clipboard |
| **R** | Toggle the prototype configuration panel |
| **Esc** | Quit |

Ordinary walking can step up at most half the collision body's height: **12 terrain voxels**
for the current 24-voxel player. A narrow ridge is not a usable foothold; normal wide stairs and
slopes remain walkable. Higher ledges require a jump within the existing jump's reach.

### Garden Tools

The bottom toolbar uses text labels with small shortcut hints. Hover to underline a tool;
selection stays gold and underlined. Click the active tool again to clear selection. The
borderless translucent bar wraps on narrow windows; tool actions and shortcuts are unchanged.

**R → GUI Appearance → GUI: panel outlines** toggles GUI panel outlines (on by default).
Click **Save** to remember it. Panel backgrounds, shadows, input controls and selection markers
remain visible; the text toolbar is always borderless.

| Input | Tool | Primary interaction |
| --- | --- | --- |
| **1** | Grow | Choose Type / variety; hold left mouse for Flora, click for Tree; right mouse removes flora in Flora mode; **Tab** changes variety |
| **3** | Dig | Left mouse removes terrain; right mouse adds terrain |
| **5** | Trim | Left mouse trims flora |
| **6** | Water | Left mouse waters soil |
| **7** | Inspector | Hover to inspect local soil |
| **8** | Till | Left mouse tills soil |
| **X** | Sprinkler | Left mouse places a sprinkler |
| **B** | Soil | Hold left mouse to pour soil; retains the main branch's soil controls |
| **N** | Stone | Left mouse places one terrain rock or slab |
| **V** | Pave | Left-drag on soil; **R → Pavement** switches paving/low walls; **Tab** changes material; **Shift + wheel** adjusts width |

### Trees

**1 / Grow → Type: Tree** offers **Apple Tree**, **Pine** and **Weeping Willow**. Click terrain
to plant one tree; **Tab** cycles varieties. Each placement receives its own repeatable seed.
The new planting selection, tuning-tree configuration and **four fresh-garden opening trees** now
use Apple Tree. The opening keeps its original planting positions and fits compact trees without
blocking the route. The old Test/Garden Tree morphology has been retired: loading those trees
(including old saves with no species field) converts them to current apples, keeping position,
ID, size, individual seed and global age. Their old baked wood is replaced, not left behind.
Loading does not overwrite the save file; use **Save** to persist the conversion. Conifer, Pine,
Willow and existing Apple descriptions retain their authoring. All architectures pass through the
[shared growth module and species rules](tree-growth.md).

New Apple Trees and the fresh opening now use a compact mature garden scale, roughly 1.5–1.8
player-body heights (seed variation applies). Pine and Willow retain their larger native planting
scales. Existing saved Apple trees keep their authored sizes; loading a garden is not a resize
operation. See [scale measurements and review](evidence/apple-tree-player-scale.md).

Apple Trees use the existing apple growth, hanging, falling and interaction systems. Pine and
Willow do not produce apples. All three share the existing pixel-leaf renderer, palette, wind,
editing and save/load systems; literal needles, pine cones and additional fruit types are not
part of this iteration.

**R → Flora → Tree** retains size, trunk thickness, seed and foliage authoring for the tuned
tree. Species-owned axes replace the old recursive branching controls where those controls no
longer apply. Save persists these settings; **Terrain & Plants → Save/Load** persists planted trees.

### Stone Placement

Press **N**, aim at terrain and click to place a stone. **R → Stone → Geometry** selects a rock
or slab and adjusts the next placement's dimensions, variation, seed and shared scale (default 0.2).
Yaw rotates the next stone around the contacted terrain normal. Each successful placement advances
the seed; existing stones do not change when parameters change.

Stones become ordinary terrain: **Dig** can edit them, and **Terrain & Plants → Save/Load**
persists them. The Debug Save button saves tool settings, not terrain. There is no permanent preview
object. Stone is unavailable in the unsaved Rooftop PoC. **B remains Soil**, not Stone.

### Pavement

Press **V** and drag with the left mouse button over soil. **Tab** switches between stone slabs,
running-bond brick and a dirt path that clears vegetation without raising terrain.
**R → Pavement** saves the surface, width, course length and seed for future gestures.

The unbuilt end follows the cursor with locally fitted curves and deformable quadrilateral
stones. Short turning tails can form an incomplete arc; they need not fit a whole corner first.
Turns that actually fold the stones or leave unusably thin pieces, existing paving and unsupported
ground show a red preview: no disconnected section is placed beyond the obstruction. Move to a legal target
to continue the same gesture; rejected targets do not become permanent guide points. Release
finishes the valid end. Right/middle click, a tool/camera switch or a blocking panel cancels the
unbuilt tail; already published terrain remains.

To make a loop, draw around and bring the cursor near the start of the **same held stroke**.
A green closure marker and **Release to close the loop** hint indicate a valid join. Release
connects the ends; move away before releasing to keep an open path. Only the unbuilt tail is
fitted, with the first built cross-section left in place. Short clicks, dirt vegetation-clearing
paths and intentional paving backdrag do not trigger closure. Very tight or obstructed joins
still require a gentler target; the tool never forces an overlap or joins a previous gesture.

Slabs and bricks are real terrain, editable with **Dig** and saved through **Terrain & Plants**.
Only the resulting terrain is saved, not editable paths or undo history. Paving is unavailable
in the Rooftop PoC. No experimental collision mode is included.

#### Low walls

In **R → Pavement**, check **Pavement: build low walls**, then use **V + left-drag**
on soil. **Tab** cycles Earth (continuous, slightly tapered soil), Stone (layered blocks)
and Brick (running-bond joints). **Shift + wheel** changes thickness; **Ctrl + wheel**
changes height independently. The panel also saves material, thickness and height.
Shared path spacing controls a wall half-brick rather than a paving course.
Uncheck the wall option to return to paving; both sets of dimensions are retained.

Material and dimensions freeze when a gesture begins; changes affect the next stroke.
Walls become ordinary terrain: **Dig** edits them, and **Terrain & Plants → Save/Load**
persists them and their vegetation. GUI **Save** saves preferences, not terrain.
Unsupported ground, existing masonry, self-overlap and world boundaries remain protected.
A new gesture on a wide enough earth-wall top can add soil height, as with ordinary soil.
Low walls use the same return-to-start closure: the last cross-section matches the first,
and masonry keeps its alternating bond and shared joint spacing at the seam. This is useful
for raised planting beds: draw the stone/brick rim, release to close it, then use **Soil** to
fill inside and **Grow** to plant. The tool does not automatically fill or clear the enclosed
area; ordinary masonry gaps and soil behavior are unchanged.

Paving backdrag and inward smoothing do not apply to walls. There are no editable wall
objects, automatic inside fill or whole-wall Undo; cancelling discards only the unbuilt tail.

## Problems and Feedback

When reporting a problem in [GitHub Issues](https://github.com/tr-nc/re-flora/issues), include:

- operating system and Re: Flora release name;
- GPU and graphics-driver version;
- what you expected and what happened;
- a screenshot, video, or log when available.

Source builds and contributor setup belong in
[Development Guide](https://github.com/tr-nc/re-flora/blob/main/docs/development.md).
