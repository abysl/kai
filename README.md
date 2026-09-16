# Kai

Kai is a virtual card table. It lets you arrange cards in a 3D space, build and
save decks, and share a table with other players. It is written in Rust and uses
Bevy, a game engine, to draw the table.

Kai is under active development. Desktop, browser, and Android clients exist;
they do not yet have identical capabilities or a finished installation experience.

## What can I do with it?

- Move cards around a free-form table, where players handle the rules themselves.
- Use a game plugin to validate actions and guide play.
- Build, import, save, and share Riftbound decks.
- Host a table or join another player using an invitation.
- Find an opponent searching for a two-player table with the same settings.
- Undo table actions with the other players' agreement.
- Add an AI opponent on desktop, Android, or in your browser, using your own
  OpenRouter/NanoGPT key or a keyless random player.

Rules support depends on the selected plugin. Kai is not an official client for
the card games it supports, and a supported deck does not guarantee that every
interaction is implemented correctly.

## Try it or contribute

You can [try Kai in your browser](https://kai.rae.blue) without installing
development tools. The browser app uses a public content gateway to load
game content; availability depends on the service and your network connection.

For Android, [download the latest APK](https://kai.rae.blue/apk/). Android may
ask you to allow installation from your browser. These are development builds;
use the version shown in Settings when reporting a problem.

This repository contains source code, not a bundled card collection.
Building from source currently requires development tools and a neighboring
checkout of the game framework.

Start with the [development guide](wiki/development.md) to build from source.
Once it is running, the [player guide](wiki/playing.md) explains the screens,
decks, invitations, and controls without assuming you know Kai.

For a bug report, include your operating system, the version shown in Settings,
what you expected, and steps to reproduce it. Do not include account keys,
invitation secrets, or private game logs.

## How the pieces fit together

Kai handles what players see and do. [Agni](https://github.com/abysl/agni) checks
and records game actions. [agni-rfb](https://github.com/abysl/agni-rfb) contains
the separately maintained Riftbound plugin.
[Spirit Library](https://github.com/abysl/spirit-library) stores and exchanges
content between devices. You do not need to understand those libraries to use
the table.

The repository split is still in progress: Kai currently reads some deck data
from Agni and its build helpers still build game modules there. See the
[development guide](wiki/development.md#related-repositories) before changing
dependencies.

## Documentation

- [Player guide](wiki/playing.md): use an already-running application.
- [Contributing](CONTRIBUTING.md): prepare your first change.
- [Development guide](wiki/development.md): setup, builds, tests, and troubleshooting.
- [Architecture](wiki/architecture.md): find the code responsible for a behavior.
- [Documentation index](wiki/README.md): specialist references and their audiences.

## License and game content

Project code is licensed under [GNU GPL version 3](LICENSE). Third-party game
names, rules, artwork, fonts, and dependencies are not automatically covered by
that license. Do not add card scans, downloaded catalogs, signing keys, or personal
stores to a contribution. Content needed during play must be obtained separately
with appropriate permission.
