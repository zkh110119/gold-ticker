# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working in this repository.

## Project

`gold-ticker` is a Rust macOS Menu Bar application for displaying the `hf_XAU` gold quote from Tencent's quote endpoint. The authoritative product and technical design is [docs/design.md](docs/design.md). It specifies the target AppKit architecture, Tencent response-validation requirements, five-minute refresh target, cached/offline behavior, threshold display rules, and release constraints.

The repository is currently at the initial Cargo application stage. The implementation roadmap below controls all feature work.

## Commands

Run commands from the repository root:

```bash
cargo check
```

```bash
cargo build
```

```bash
cargo test
```

```bash
cargo test <test_name>
```

```bash
cargo run
```

```bash
cargo fmt --check
```

```bash
cargo clippy --all-targets -- -D warnings
```

`cargo run` currently runs the initial Rust binary; once the AppKit application is implemented, it will launch the macOS Menu Bar app.

## Intended Architecture

Keep the application separated into these dependency directions:

```text
AppKit UI (status item, popover, settings)
        ↓
App controller (lifecycle and UI state coordination)
        ↓
Refresh/settings/cache services
        ↓
MarketDataProvider abstraction
        ↓
Tencent hf_XAU HTTP provider
```

- UI code must not issue HTTP requests or parse market responses.
- Provider implementations must not depend on AppKit.
- Price-change and threshold logic must remain independent of UI, networking, and file storage so it can be tested deterministically.
- Use `rust_decimal::Decimal` for prices, thresholds, and percentage calculations; do not use `f64` for price comparisons.
- The top Menu Bar status item must not rely on red/green color. Use neutral text plus arrows/symbols there; show red/green threshold and direction styling in the Popover.
- Treat `https://qt.gtimg.cn/q=hf_XAU&_={timestamp}` as an untrusted third-party response. Do not execute its JavaScript-like payload. Parse only the expected response form, validate all required fields, and keep response-field mapping confined to the Tencent provider.

## Mandatory ROADMAP

Execute this roadmap strictly in order. Do not start, partially implement, or merge a later phase before the preceding phase is complete and validated. At the completion of each phase, update its **Status** in this table, report the validation results, and ask the user for explicit approval before beginning the next phase.

Status values are limited to `Not started`, `In progress`, `Awaiting approval`, `Completed`, and `Blocked`. Update the table immediately when a phase begins, becomes blocked, awaits approval, or is completed. A phase may be marked `Completed` only after its completion gate passes and the user has approved moving beyond it. After the user confirms that an R phase is complete, automatically write an appropriate commit log, commit all changes for that phase, and push the commit to GitHub.

| Status | Phase | Scope | Completion gate |
| --- | --- | --- | --- |
| Completed | R0 — Interface verification | Captured and decoded a live `hf_XAU` fixture; confirmed response mapping for current price, percentage change, intraday high/low, quote time/date, previous close, and display name. Currency and quote unit remain unconfirmed by the response and must be verified before user-facing labeling. | Parser fixture test validates the confirmed mapping. |
| Completed | R1 — Application foundation | Added pinned AppKit bindings, an accessory `NSApplication` lifecycle, retained `NSStatusItem`, neutral mock status text/tooltip, and a clickable native mock `NSPopover`. A local smoke test confirmed the run loop launches without an immediate runtime error. | Build succeeds and the native Menu Bar item renders mock state. |
| Completed | R2 — Market data and persistence | Added a Tencent `hf_XAU` provider with a 10-second total timeout, GB18030 decoding, response-size limit, normalized decimal quote mapping, atomic JSON cache, initial background fetch, five-minute refresh target, and capped `5 → 10 → 20 → 40 → 60` minute failure backoff. The AppKit UI reads cached data at launch and receives refresh results on its main run loop. | Provider, cache, and refresh tests pass; cached/expired state works manually. |
| Completed | R3 — User-facing quote behavior | Added exact Decimal price-change and threshold rules; a persisted, zero-default non-negative threshold; grouped price/change formatting; native Popover quote, threshold, freshness, and error presentation; a 15-second manual-refresh debounce; and immediate manual worker triggering. Menu Bar status remains neutral with arrows/symbols, while the Popover pairs threshold/direction colors with text. Currency and quote unit remain explicitly unconfirmed. | Threshold, change, formatting, settings, refresh, and UI-state tests pass; app launches without an immediate runtime error. Native visual interaction remains required on a logged-in macOS desktop session. |
| Completed | R4 — Reliability and quality | Added redacted, bounded rotating diagnostics; shared durable atomic storage with cleanup; automatic recovery that quarantines malformed cache/settings files; refresh lifecycle events; and semantic AppKit colors that preserve appearance-adaptive Popover styling while keeping Menu Bar text neutral. | `cargo fmt --check`, `cargo test` (28 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo build` pass. Native smoke testing plus user-verified light/dark Popover readability and long-running refresh behavior passed. |
| Completed | R5 — GitHub Releases distribution | Built an unsigned, unnotarized `.app` and `.dmg` for manual upload to GitHub Releases, including original app assets, a SHA-256 sidecar, privacy/disclaimer material, and GitHub Issues support. No Mac App Store submission, sandboxing, Developer ID signing, or notarization steps were added. The initial project commit and GitHub repository community materials are published. Public publication of a release remains contingent on confirming Tencent data-source terms, rate limits, display/re-distribution authorization, and regional availability. | `cargo fmt --check`, `cargo test` (28 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` pass; the DMG and checksum were produced; `Info.plist`, bundle resources, executable permissions, DMG contents, and a secret scan passed. The mounted DMG contents were validated and `GoldTicker.app` launched then exited cleanly. Gatekeeper/notarization validation is intentionally unavailable for this unsigned distribution model. |
| Completed | R6 — Popover dismissal behavior | Configured the native quote `NSPopover` with transient behavior so AppKit dismisses it after an outside click while retaining status-item toggling and the existing refresh/threshold controls. Added an automated policy test. Also made diagnostics-test temporary directories process-unique after a concurrent test cleanup collision was exposed during validation. | `cargo fmt --check`, `cargo test` (29 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` pass. The app launches and exits cleanly. A logged-in macOS desktop check confirmed outside-click dismissal, threshold editing/saving, manual refresh, and status-item toggling all work. |
| Completed | R7 — Startup and threshold alerts | Added a persisted, default-on startup preference and native Popover status that accurately disables the feature for the selected unsigned DMG distribution; macOS requires code signing to register an `SMAppService` login item. Added persisted threshold-side state and local-notification handling that requests authorization only after a fresh at-or-above → below transition, and deduplicates alerts while the quote remains below. | `cargo fmt --check`, `cargo test` (32 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` pass. The app launches and exits cleanly. The logged-in desktop verification confirmed the disabled 开机启动 control and unsigned-build explanation; notification authorization and threshold-crossing behavior were verified, including no duplicate alerts while below, recovery then re-crossing, persistence across restart, and no prompt/alert for startup, stale data, no data, or fetch failures. |
| Completed | R8 — Threshold input grouping separators | Fixed the threshold editor so a user can edit and save an initially formatted value such as `4,200` to `4,201` without manually removing grouping separators. Kept strict non-negative Decimal validation and accepted only valid grouping syntax rather than silently stripping malformed commas. | `cargo fmt --check`, `cargo test` (35 passed), `cargo clippy --all-targets -- -D warnings`, and `cargo build --release` pass. Parsing tests cover formatted integers and decimals, malformed separator placement remains rejected, persisted threshold round-trips correctly, successful threshold saves reformat the editor with grouping, and native Popover verification confirmed an edited grouped value saves and renders correctly. |

## Roadmap Change Rule

Every new feature, behavior change, integration, or scope expansion must be added to the ROADMAP before implementation. Place it in the correct phase, or add a new sequential phase after the current approved roadmap. Describe its scope and completion gate, then obtain the user's explicit approval before writing implementation code. Do not bypass this rule for seemingly small features.

## Validation Expectations

For each implemented roadmap phase, run the most relevant checks before requesting approval. At minimum, run `cargo fmt --check`, the targeted tests, and `cargo clippy --all-targets -- -D warnings` when dependencies and targets support them. Record any platform-specific manual verification required by the phase.
