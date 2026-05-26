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

Push a `v*` tag to build release packages automatically:

```bash
git tag v0.2.0
git push origin v0.2.0
```

Download the finished packages from the workflow run's **Artifacts** section.

## Runtime layout

Each package contains the executable plus runtime data directories:

```text
re-flora(.exe)
assets/
config/
shader/
README.md
LICENSE
LICENSE-ASSETS
BUILD_INFO.txt
```

Release packages also bundle non-system native libraries needed by the binary:

- Windows: `phonon.dll` next to `re-flora.exe`.
- macOS: `lib/libphonon.dylib`, `lib/libshaderc_shared*.dylib`, `lib/libvulkan.1.dylib`, `lib/libMoltenVK.dylib`, and `vulkan/icd.d/MoltenVK_icd.json`.
- Fedora/Linux: `lib/libphonon.so` and `lib/libshaderc_shared.so*` with the executable rpath set to `$ORIGIN/lib`.

The CI release build uses `--features no_validation_layer`, so players do not need Vulkan validation layers or the Vulkan SDK just to run the package.

Users still need a Vulkan-capable GPU and a working GPU driver. Windows and Linux drivers normally provide the Vulkan driver/ICD. macOS packages include MoltenVK for Vulkan-on-Metal.

The executable resolves resources from `RE_FLORA_ROOT`, the current directory, or the executable's package directory, so unpacked itch downloads can run outside the source checkout.
