## GoldTicker v<version>

### Download

- **macOS:** 13.0 or later
- **Architecture:** `<architecture>`
- **DMG:** `GoldTicker-<version>-<architecture>.dmg`
- **SHA-256:** verify with the accompanying `.dmg.sha256` file

### Installation

1. Download both the DMG and its checksum sidecar.
2. Verify the checksum.
3. Open the DMG and move `GoldTicker.app` to `/Applications`.

> This release is unsigned and unnotarized. macOS will identify it as unverified, and standard Gatekeeper verification is unavailable. Download only from this repository and verify the checksum before opening it.

### Notes

- This unsigned release cannot enable startup at login; macOS requires a code-signed app for that feature.
- A threshold alert requests macOS notification permission only after a fresh price crossing from at-or-above the saved threshold to below it; it does not repeat while the quote remains below.
- Quotes are sourced from Tencent's `hf_XAU` endpoint and may be delayed, unavailable, or inaccurate.
- Currency and quote unit remain unconfirmed by the upstream response.
- Gold Ticker is for informational reference only, not investment or trading advice.

Read the [privacy notice](../blob/main/docs/PRIVACY.md) and [market-data disclaimer](../blob/main/docs/DISCLAIMER.md). For support, use [GitHub Issues](../../issues).
