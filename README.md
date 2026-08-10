# Re: Flora

> Cultivate a living voxel garden at your own pace.

![Re: Flora](./demo/img/splash.png)

Re: Flora is an experimental, full-screen gardening game about shaping a small natural place,
planting freely, and seeing the world respond. It is designed as an active but low-pressure escape:
the garden asks for your attention while you are inside it, but it does not ask you to organize your
life around it.

## Download

**[Download the latest Re: Flora release](https://github.com/tr-nc/re-flora/releases/latest)**

The latest release page provides packaged builds for Windows, macOS, and Fedora. Download the archive
for your platform, extract the complete folder, and run Re: Flora from inside it.

Each archive contains the README and playing guide from the release that produced it. The `main`
branch may describe interactions that are not in the latest tagged package yet, so follow the guide
inside the downloaded archive when it differs from this page.

You need a Vulkan-capable GPU with a current graphics driver. RTX hardware, the Vulkan SDK, Rust, and
other development tools are not required to play a packaged release. The macOS package includes its
Vulkan-on-Metal runtime.

See [Playing Re: Flora](./docs/playing.md) for platform launch notes, controls, and a short guide to
the current prototype.

## Current Development State

The current development build is an evolving prototype. It can:

- inhabit and explore a low-resolution 3D voxel island;
- shape and smooth terrain;
- grow, arrange, trim, and remove flora without a visible placement grid;
- water, fertilize, till, and inspect local soil;
- place trees, sprinklers, and irrigation pipes;
- experience dynamic light, wind, water, particles, wildlife, and spatial sound.

The complete care, harvest, and seed-circulation loop is the current product milestone, not a claim
about the latest packaged release. See
[First Garden Moment](https://github.com/tr-nc/re-flora/blob/main/docs/first_garden_moment.md) for that
scope.

## Start Exploring

After launch:

1. Use **WASD** or the arrow keys and the mouse to look around the garden.
2. Press **G** to switch between the orbit editing camera and free-look movement.
3. Press **2** to select Grow, use **Tab** to choose a plant, then hold the left mouse button over
   soil.
4. Press **6** to select Water and hold the left mouse button over the planted area.
5. Press **Esc** whenever you want to leave.

The full control reference is in [Playing Re: Flora](./docs/playing.md#controls).

## Direction

Re: Flora is being shaped as a Restorative Garden for adults who want a deliberate break from modern
work:

> I spent time with my garden. It responded to me, and it became more mine.

Read the canonical
[game direction](https://github.com/tr-nc/re-flora/blob/main/docs/game_direction.md) and current
[product roadmap](https://github.com/tr-nc/re-flora/blob/main/ROADMAP.md).

## Open Source and Feedback

Re: Flora is open source. Player impressions, bug reports, screenshots, and videos are welcome in
[GitHub Issues](https://github.com/tr-nc/re-flora/issues).

If you want to build the project or contribute code, read
[CONTRIBUTING.md](https://github.com/tr-nc/re-flora/blob/main/CONTRIBUTING.md). Local build setup and
engineering validation live in
[Development Guide](https://github.com/tr-nc/re-flora/blob/main/docs/development.md), outside this
player-facing README.

## Acknowledgements

- [egui-ash-renderer](https://github.com/adrien-ben/egui-ash-renderer)
- TheMaister, Khronos Group, and the broader graphics-programming community

## License

Re: Flora is dual-licensed:

- **Code:** [GPL-3.0](./LICENSE)
- **Assets:** [CC BY-NC-SA 4.0](./LICENSE-ASSETS)
