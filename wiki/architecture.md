# Kai architecture

Audience: developers who have built Kai and want to locate or change a feature.
Read the [development guide](development.md) first; Bevy expertise is not assumed.

## One request, one accepted result

The application has three responsibilities: present a game, collect a player's
request, and show the result accepted by the game framework.

A drag produces a message such as `CardDropped`. The networking layer routes
that request to the local or remote host. Agni validates and records it. Kai
then updates its visible cards from the resulting table view. Animations may
use frame time and floating-point coordinates; game decisions may not.

This separation matters because every player must agree on the result even if
their device renders a different number of frames.

## Where to start

| Change | Start here | Responsibility |
|---|---|---|
| Startup and application registration | `src/app.rs` | Initialize the application and its systems |
| Home, lobby, and deck selection | `src/menu/` | Screens and navigation |
| Deck editing and persistence | `src/deck/` | Drafts, catalogs, imports, and saved decks |
| Card layout, gestures, and animation | `src/table/` | Render and interact with the accepted view |
| Multiplayer and joining | `src/net/` | Route requests and consume session events |
| Native automated opponents | `src/ai/` | Drive a seat through normal game requests |
| Platform services | `src/os/` | Target-specific clipboard, storage, and integration |

`src/lib.rs` is the module entry point, not a place for new application logic.
`src/main.rs` delegates startup to the library.

## Vocabulary

A **seat** is one player's position in a session. A **zone** is a location such
as a hand or discard pile. An **intent** is a requested action. The **host**
orders accepted actions. A **view** contains what a client should render.
A **plugin** supplies game-specific decisions and available actions.

Agni owns the simulation, session, and wire formats. Spirit Library supplies
content storage and peer communication. Neither library should depend on Kai.

## Before changing a boundary

- UI changes need checks at phone and desktop sizes and with touch and pointer input.
- Session changes need host/client and reconnect tests, not just a UI test.
- Hidden-information changes need checks from every affected seat.
- Browser builds use a different transport/runtime path from native builds.
- Game modules are identified by their hardened bytes and pinned when play starts.

The [documentation index](README.md) points to detailed references by subsystem.
