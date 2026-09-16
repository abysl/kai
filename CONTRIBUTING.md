# Contributing to Kai

This guide is for programmers who know how to edit code and use Git but have
not worked on Kai. Rust and Bevy experience helps, but you do not need to know
the entire application before making a small change.

## Make your first change

1. Read the [project introduction](README.md) and follow the
   [development guide](wiki/development.md).
2. Create a branch for one bug or feature. For a larger change, open an issue
   describing the behavior you want before implementing it.
3. Find the relevant component using the [architecture map](wiki/architecture.md).
4. Add a regression test for changed logic. For a visual change, record the
   viewport size, input method, and before/after screenshots using content you
   may share.
5. Run the [PR checks](wiki/development.md#checks) and the tests relevant to your
   change. Report checks you could not run.
6. Open a pull request on GitHub with the problem, approach, and verification.

## Boundaries to preserve

A card drag is a request, not permission to change the game state. Kai sends
requests to Agni and renders the accepted result. Fix rule validation in Agni
or the relevant game plugin, not in an animation or click handler.

Keep desktop, browser, and Android differences behind the existing platform
interfaces. A headless feature does not remove the renderer's dependencies.

Code uses descriptive names and small functions instead of comment lines.
Put explanations of APIs in the wiki, design reasons in design documents, and
change-specific reasoning in the commit message.

Do not add credentials, internal deployment configuration, game artwork, or
downloaded card databases. Use synthetic fixtures where possible and record
the source and license of any third-party material you propose adding.

## Compatibility and review

Include snapshot or protocol effects in your PR description. Game peers must
agree on the wire protocol and pinned game modules; changing a message is not
just a UI change. Maintainers coordinate version bumps and releases.

A documentation-only change does not require an application version bump.
A change shipping a new application build must bump the package version.
Read [AGENTS.md](AGENTS.md) for additional implementation constraints.
