# Contributing to Kai

Kai is the renderer and client for Agni. It owns presentation, input,
platform integration, pairing, and transport routing. It must never mutate
authoritative game state; interactions emit requests for Agni to validate.

## Development

```text
direnv allow
dev
unit-test
fmt-check
clippy
```

Use `run` for a static release-style build. Use `web-build` for the browser
bundle. Android builds run from the `android/` development environment.

Read [wiki/design/table.md](wiki/design/table.md) before changing the table,
camera, interaction, or layout code. Read the Agni API and session documents
before changing network routing or session integration.

Card art, plugins, and hydrated stores are runtime inputs. Do not commit card
art, generated modules, local stores, credentials, or build outputs.

## Pull requests

Use a focused branch and include the platform scope of the change. Run the
relevant desktop, web, or Android check locally and include manual playtest
steps for visual or interaction changes.
