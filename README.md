# Gold Ticker

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Gold Ticker is a native macOS Menu Bar application that displays the Tencent `hf_XAU` quote. It uses a compact neutral Menu Bar title and a Popover for quote details, threshold configuration, data freshness, and manual refresh.

> **Distribution status:** GitHub Releases provide an ad-hoc signed, unnotarized DMG for manual installation. This project is not distributed through the Mac App Store. macOS will identify the release as unverified; only download an asset whose source and checksum you have verified.

## Requirements

- macOS 13.0 or later
- Apple Silicon or Intel Mac matching the release artifact architecture
- Internet access to Tencent's `hf_XAU` endpoint

The app is not a trading tool and does not provide investment advice. Read [the disclaimer](docs/DISCLAIMER.md) and [privacy notice](docs/PRIVACY.md) before use.

## Install a GitHub Release

1. Download the DMG and its `.sha256` sidecar from the matching [GitHub Release](https://github.com/zkh110119/gold-ticker/releases).
2. Verify the downloaded DMG against its checksum.
3. Open the DMG and move `GoldTicker.app` to `/Applications` using the included Applications alias.
4. Launch the app only after reviewing the macOS security information and confirming that the asset came from this project.

Because releases are ad-hoc signed but unnotarized, standard Gatekeeper verification is unavailable. The project does not provide instructions for bypassing macOS security protections.

Before publishing any release, the publisher must document Tencent data-source terms, rate limits, display rights, and redistribution authorization. Report questions or issues through [GitHub Issues](../../issues).

## Build locally

```bash
cargo build
```

Run the development binary:

```bash
cargo run
```

Run checks:

```bash
cargo fmt --check
```

```bash
cargo test
```

```bash
cargo clippy --all-targets -- -D warnings
```

## Build a release DMG

Generate the icon once after cloning:

```bash
./packaging/generate-icon.sh
```

Build an unsigned, unnotarized app bundle, DMG, and SHA-256 sidecar:

```bash
./packaging/build-app.sh
```

The artifacts are written to `dist/GoldTicker.app`, `dist/GoldTicker-<version>-<architecture>.dmg`, and `dist/GoldTicker-<version>-<architecture>.dmg.sha256`. See [the release guide](docs/RELEASE.md) for the manual checklist and the tag-triggered GitHub Actions release workflow.

## Data and limitations

- Quotes are requested from Tencent's `hf_XAU` endpoint every five minutes after startup, with failure backoff.
- The application keeps local settings (including its threshold-alert state and startup preference), a recent quote cache, and redacted bounded diagnostics in macOS Application Support. It does not upload those files.
- A threshold alert requests macOS notification permission after a fresh quote crosses from at-or-above the saved threshold to below it. The first saved threshold establishes the current quote as the baseline, so a quote that is already below a newly saved threshold does not alert. It does not alert repeatedly while the quote remains below the threshold.
- Primary-clicking the Menu Bar item opens or closes the Popover, clicking outside dismisses it, and secondary-clicking opens a menu with `退出 Gold Ticker`.
- The startup preference is shown but unavailable in the unsigned DMG. macOS requires a code-signed app to register a login item.
- The upstream response does **not** yet establish a currency or quote unit. Gold Ticker deliberately displays this as unconfirmed instead of guessing.
- Quote availability, accuracy, delay, geographic access, rate limits, and redistribution rights belong to the third-party data source and are not guaranteed by this application.

## License

Licensed under the [MIT License](LICENSE).
