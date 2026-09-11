# GitHub Releases DMG Guide

Gold Ticker is distributed as an **ad-hoc signed, unnotarized DMG** through GitHub Releases. It is not distributed through the Mac App Store.

## Publication prerequisites

Do not publish until all of the following are true:

1. The publisher has reviewed and recorded Tencent endpoint terms, rate limits, regional availability, display rights, and redistribution/commercial authorization.
2. The quote currency and unit have been independently confirmed, or release documentation clearly retains their current unconfirmed status.
3. The privacy notice, disclaimer, and [GitHub Issues support channel](../../../issues) are current.
4. The publisher accepts that macOS will identify this artifact as unverified because it is not signed with Developer ID or notarized.

## Build the release artifact

Generate the committed icon resource if it is not already present:

```bash
./packaging/generate-icon.sh
```

Run the required quality checks:

```bash
cargo fmt --check
```

```bash
cargo test
```

```bash
cargo clippy --all-targets -- -D warnings
```

Build the app bundle, DMG, and checksum sidecar:

```bash
./packaging/build-app.sh
```

The command creates an ad-hoc signed app bundle, DMG, and checksum sidecar:

- `dist/GoldTicker.app`
- `dist/GoldTicker-<version>-<architecture>.dmg`
- `dist/GoldTicker-<version>-<architecture>.dmg.sha256`

## Inspect the artifact

Before uploading, inspect the generated `Info.plist`, icon resource, executable permissions, and DMG contents. Mount the DMG, confirm it contains `GoldTicker.app` and the `Applications` alias, then launch the app in a local macOS session.

Verify the published checksum from the directory containing both downloaded files, replacing the filename with the generated artifact name:

```bash
cd dist && shasum -a 256 -c GoldTicker-<version>-<architecture>.dmg.sha256
```

Check that neither the DMG nor its source staging content contains credentials, quote caches, thresholds, or diagnostic logs.

## Automated GitHub Release

The repository includes [`.github/workflows/release.yml`](../.github/workflows/release.yml). Push a tag matching the exact version in `Cargo.toml`, for example `v0.1.0`, to start the macOS release workflow:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow runs on `macos-14`, validates formatting, tests, Clippy, and the release build, checks that the tag matches the package version, builds and verifies the unsigned DMG and checksum, then creates the GitHub Release with only the matching `.dmg` and `.dmg.sha256` assets. The `release` environment can be configured in repository settings to require an explicit maintainer approval before publication.

The workflow does not bypass the publication prerequisites above. Confirm Tencent data-source terms, rate limits, regional availability, display rights, redistribution authorization, and the current privacy/disclaimer material before pushing a release tag. It does not sign or notarize the app, submit to the Mac App Store, or provide Gatekeeper bypass instructions.


1. Create a GitHub Release using the version from `Cargo.toml` as its tag and title.
2. Upload only the matching `.dmg` and `.dmg.sha256` files.
3. State the supported macOS version and CPU architecture in the release notes.
4. State that the asset is unsigned and unnotarized, link to [the privacy notice](PRIVACY.md), [the disclaimer](DISCLAIMER.md), and [GitHub Issues](../../../issues), and do not claim Gatekeeper approval.
5. Publish only after the publication prerequisites above have been satisfied.

## Release checklist

- [ ] `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` pass.
- [ ] The DMG opens, includes `GoldTicker.app` and an Applications alias, and passes `hdiutil verify`.
- [ ] `Info.plist`, icon resource, and executable permissions are correct.
- [ ] The SHA-256 sidecar verifies the exact uploaded DMG.
- [ ] No secrets, credentials, raw quote responses, caches, thresholds, or user diagnostics are in the release asset.
- [ ] The unsigned release makes no claim that startup-at-login is available; it requires a code-signed app.
- [ ] Threshold-alert permission is requested only after a fresh downward threshold crossing. On macOS, the app appears under System Settings > Notifications only after this authorization request; verify the packaged app's notification entry and delivered alert.
- [ ] Privacy notice, disclaimer, support channel, and third-party market-data authorization are current.
