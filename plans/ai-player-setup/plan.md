# AI player setup

Audience: contributors implementing and reviewing the table-creation flow.

## Outcome

Offer an AI player during table creation on desktop, Android, and browser.
Open a settings sheet with OpenRouter/NanoGPT, a masked session-only API key,
and a searchable dropdown populated from the provider's tool-capable models.
Require confirmation before creating or adding a model-backed seat. Keep a
keyless random opponent available.

## Approach

1. Introduce provider-aware credentials and HTTP/model discovery without
   persisting credentials, logging response bodies, or blocking UI frames.
2. Route every add/play-vs-AI entry point through the responsive settings sheet.
3. Reuse the seat replica, legal commands, and model prompts in a browser
   runner that yields during network requests and between accepted actions.
   Cancel stale decisions on stop, reset, rollback, or replacement.
4. Verify native and browser compilation, provider parsing and validation,
   settings navigation, secret handling, and browser seat lifecycle. Update
   player and developer documentation with costs and privacy behavior.

## Constraints

Keys go directly to the selected provider. There is no server-side AI proxy.
Model lists are live metadata, not proof of key validity or available credit.
AI requests must use the AI seat's replica, never the human host's private view.
Keep R as reveal and the existing undo shortcuts unchanged.

## Implementation

Implemented for version 0.18.0 on `feat/ai-player-setup`. The common lobby
sheet and provider-aware native client are connected, and browser seats use
an asynchronous runner over the existing ClientSession and host message path.
Player instructions and implementation details are in the wiki. See
[review](review/implementation.md) for verification and remaining manual checks.
