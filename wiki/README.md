# Documentation guide

Audience: readers choosing a starting point and contributors editing documentation.

Start with the [project introduction](../README.md). You should not need to
know the project's other libraries, development history, or maintainer's
environment to understand it.

## Choose a document

| Document | Intended reader | What it provides |
|---|---|---|
| [Kai implementation rules](../AGENTS.md) | Coding assistants and implementation contributors | Constraints to preserve while editing |
| [Contributing to Kai](../CONTRIBUTING.md) | New contributors with basic programming knowledge | Prepare, test, and submit a change |
| [Kai](../README.md) | First-time visitors | What the project does, limitations, and where to start |
| [Android connectivity harness](../tests/android/README.md) | Developers testing platform integration | Prerequisites, commands, results, and cleanup |
| [Run a two-client connectivity check](../tests/connectivity/README.md) | Developers testing platform integration | Prerequisites, commands, results, and cleanup |
| [Kai architecture](architecture.md) | Developers new to the codebase | Responsibilities, vocabulary, and code navigation |
| [Loading card images](design/assets.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Automated seats and waiting](design/bot.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Connectivity test design](design/connectivity.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Deck editing](design/deck-editor.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Saved decks and identity](design/deck-history.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [The game log boundary](design/deterministic-log.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Multiplayer integration](design/multiplayer.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Presenting connection information](design/peers.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Rendering and table interaction](design/table.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Interface design](design/ux.md) | Contributors to this subsystem | Read the stated prerequisites, then use as a focused reference |
| [Developing Kai](development.md) | Programmers new to the project | Install tools, build, test, and troubleshoot |
| [Playing on a Kai table](playing.md) | Players | Use an already-running application |

## Writing for the reader

Introductions explain the problem, capabilities, limitations, and next step.
They expand project-specific names before using them and do not double as
reference manuals.

Contributor guides assume basic programming and Git, not knowledge of this
codebase. Setup instructions name prerequisites, the working directory, the
command, the expected result, and common failures.

Subsystem references can assume the linked introductory material, but should
state that prerequisite. Explain why a boundary exists before listing internal
symbols. Distinguish implemented behavior from a proposal.

Specifications serve compatibility work: preserve precise contracts and
explicit status markers. A prose rewrite must not silently change a protocol.

Machine-readable fixtures are data even when their extension is Markdown.
Do not paraphrase or reflow data files as part of a documentation edit.
Keep credentials, personal paths, and internal deployment details out of
public examples. Use local or example addresses when showing configuration.
