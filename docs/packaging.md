# Packaging paid builds

Official binaries are built by GitHub Actions and should be distributed through itch.io, not GitHub Releases.

## Repository setup

Add these in GitHub repository settings:

- Secret `BUTLER_API_KEY`: an itch.io API key for `butler`.
- Variable `ITCH_PROJECT`: itch project slug, for example `username/re-flora`.
- Optional variable `ITCH_PUBLISH_ON_TAG`: set to `true` only if `v*` tags should auto-publish.

## Manual test on `main`

Run **Actions → itch builds → Run workflow** on `main` with `publish_to_itch` left `false`.

That performs release builds for:

- `windows`
- `macos`
- `fedora`

It packages each build, prints the package name and size, and does not upload GitHub artifacts or public releases.

## Publish to itch.io

Run the same workflow with `publish_to_itch` set to `true`, or push a `v*` tag after setting `ITCH_PUBLISH_ON_TAG=true`.

The workflow pushes packages directly to itch.io channels:

- `username/re-flora:windows`
- `username/re-flora:macos`
- `username/re-flora:fedora`

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
