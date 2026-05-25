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

The executable resolves resources from `RE_FLORA_ROOT`, the current directory, or the executable's package directory, so unpacked itch downloads can run outside the source checkout.
