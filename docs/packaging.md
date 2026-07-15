# Packaging release builds

Official binaries are built by GitHub Actions and attached to the workflow run as downloadable artifacts.

## Manual test on `main`

Run **Actions → release packages → Run workflow** on `main`.

That performs release builds for:

- `windows`
- `macos`
- `fedora`

It packages each build, prints the package name and size, and uploads each zip as a workflow artifact retained by GitHub Actions.

## Tag release

Use the release tag helper from a clean, up-to-date `main` checkout:

```bash
scripts/release_tag.py
```

The default tag is `v<Cargo.toml package.version>`. After bumping `Cargo.toml` and `Cargo.lock`, preview or cut an explicit patch version:

```bash
scripts/release_tag.py --dry-run 0.2.8
scripts/release_tag.py 0.2.8
```

The helper creates an annotated `v*` tag and pushes it to `origin`, which starts the package workflow automatically. It refuses dirty worktrees, non-`main` branches, stale branches, existing tags, and versions that differ from `Cargo.toml` unless explicitly overridden.

Manual fallback:

```bash
git tag -a v0.2.8 -m "Re: Flora 0.2.8"
git push origin v0.2.8
```

Download the finished packages from the workflow run's **Artifacts** section. Tag builds also create or update the matching GitHub Release with package assets.

## Runtime layout

Each package contains the executable plus runtime data directories:

```text
re-flora(.exe)
assets/
config/
README.md
LICENSE
LICENSE-ASSETS
BUILD_INFO.txt
```

Release packages also bundle non-system native libraries needed by the binary:

- Windows: `phonon.dll` next to `re-flora.exe`.
- macOS: `lib/libphonon.dylib`, `lib/libvulkan.1.dylib`, `lib/libMoltenVK.dylib`, and `vulkan/icd.d/MoltenVK_icd.json`.
- Fedora/Linux: `lib/libphonon.so` with the executable rpath set to `$ORIGIN/lib`.

GLSL sources and shaderc are build-time inputs. Optimized and reflection SPIR-V artifacts are embedded in the executable, so release packages do not include `shader/` or a shaderc runtime library.

The CI release build uses `--features no_validation_layer`, so players do not need Vulkan validation layers or the Vulkan SDK just to run the package.

Users still need a Vulkan-capable GPU and a working GPU driver. Windows and Linux drivers normally provide the Vulkan driver/ICD. macOS packages include MoltenVK for Vulkan-on-Metal.

The executable resolves resources from `RE_FLORA_ROOT`, the current directory, or the executable's package directory, so unpacked itch downloads can run outside the source checkout.
