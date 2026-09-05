# Playing Re: Flora

This guide explains how to download, launch, and explore the current Re: Flora prototype. The latest
release is an evolving systems playground; the complete care, harvest, and Seed Circulation loop in
[First Garden Moment](https://github.com/tr-nc/re-flora/blob/main/docs/first_garden_moment.md) is still
the current product milestone.

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

## A First Visit

The current build has prototype controls rather than a finished tutorial. This short sequence shows
the most relevant garden interactions:

1. Move through the world and find an area of exposed soil.
2. Press **2** for Grow. Press **Tab** to cycle between available flora, then hold the left mouse
   button over soil to plant.
3. Press **6** for Water and hold the left mouse button over the planted area. Watch the soil and
   growth response.
4. Press **7** for the soil inspector and hover over terrain to inspect moisture.
5. Try **3** to dig, **4** to smooth, **5** to trim, or **8** to till.
6. Press **X** to place a sprinkler or **V** to draw an irrigation pipe.
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

### Garden Tools

| Input | Tool | Primary interaction |
| --- | --- | --- |
| **1** | Hand | Clear the active tool |
| **2** | Grow | Left mouse plants; right mouse removes flora; **Tab** changes flora |
| **3** | Dig | Left mouse removes terrain; right mouse adds terrain |
| **4** | Smooth | Left mouse smooths terrain |
| **5** | Trim | Left mouse trims flora |
| **6** | Water | Left mouse waters soil |
| **7** | Inspector | Hover to inspect local soil |
| **8** | Till | Left mouse tills soil |
| **Z** | Tree | Left mouse places a tree |
| **X** | Sprinkler | Left mouse places a sprinkler |
| **V** | Pipe | Left mouse begins or completes a pipe; right mouse cancels |

## Problems and Feedback

When reporting a problem in [GitHub Issues](https://github.com/tr-nc/re-flora/issues), include:

- operating system and Re: Flora release name;
- GPU and graphics-driver version;
- what you expected and what happened;
- a screenshot, video, or log when available.

Source builds and contributor setup belong in
[Development Guide](https://github.com/tr-nc/re-flora/blob/main/docs/development.md).
