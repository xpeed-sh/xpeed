# Localization / Idiomas

Xpeed 0.7.1 starts in English (`en`). Select **Settings → Language → Español** to switch to Spanish (`es`). The preference is stored as `language` in `settings.json`; older settings and unknown language codes default to English while preserving the other preferences. Changes apply without restarting or disconnecting sessions.

## Application catalogs

The shared `xpeed-i18n` crate contains `locales/en.json` and `locales/es.json`. Each stable message key appears in both files. `t(message)` selects a translated message; `format(message, values)` substitutes preformatted values in one pass. Keep all template placeholders identical in each language. User paths, terminal output, command contents, credentials and provider responses never enter the translation system.

To add another language, create its catalog with the same keys, register it in `Language` (including code, native name, catalog and `ALL`), and extend language selection/storage. English remains the fallback. The catalogs are compiled into Xpeed, so changing a catalog requires rebuilding. Catalog tests verify key and placeholder parity. Use stable message keys rather than changing them when revising translations.

## Extension manifests

Default manifest text and `README.md` should be English. Supply optional per-language metadata in `localizations`:

```json
{
  "name": "Server monitor",
  "description": "Live server metrics",
  "capabilities": ["serverMetrics", "localization"],
  "localizations": {
    "en": {
      "name": "Server monitor",
      "description": "Live server metrics",
      "readme": "README.md"
    },
    "es": {
      "name": "Monitor del servidor",
      "description": "Métricas del servidor en tiempo real",
      "readme": "README.es.md",
      "views": {"monitor": "Monitor"}
    }
  }
}
```

This is a fragment; the remaining manifest fields are still required. A locale can also provide `permissions` for display. Localized permission labels never replace the actual permission identifiers used by the host. View keys refer to stable view IDs; translation does not change IDs, panel keys or JSON-RPC methods. Documentation paths must be relative Markdown files inside the repository. Git metadata fetches these documents for offline use. Metadata without the selected locale falls back to English/default manifest values.

## Extension processes

Xpeed sends `language: "en"` or `"es"` in `initialize` alongside the existing protocol/client fields. Providers should default to English when this field is absent or unsupported. An extension declaring the `localization` capability receives the notification:

```json
{"jsonrpc":"2.0","method":"locale/change","params":{"language":"es"}}
```

Update interface/status text and emit refreshed view content. Keep conversations, user input, process identifiers and metrics intact; do not disconnect or reset charts merely to translate them. Other extensions may ignore the optional initialize field. The host only sends live language notifications to extensions that opt in.

Codex and Server monitor share `xpeed-i18n`. Standalone projects prepared by `scripts/prepare-git-repositories.ps1` include it under `vendor/xpeed-i18n`, so each project can compile independently. Other extensions can use their own localization libraries and implement this same protocol.

## Verification

Run `cargo test --workspace --locked` and `cargo clippy --workspace --locked --all-targets -- -D warnings`. For isolated UI captures use `--capture artifacts/english.png --language en` or `--capture artifacts/spanish.png --language es`, optionally with `--show-settings`, `--demo-extension xpeed.codex` or `--demo-monitor artifacts/monitor-test/dashboard.json`. Capture runs do not save user settings.
