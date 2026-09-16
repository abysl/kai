# Playing on a Kai table

Audience: players with an installed or running copy of Kai. No programming
knowledge is needed. For installation from source, use the
[development guide](development.md).

## Start a table

Kai opens without dealing cards. Choose a game on the home screen, then use
its lobby to choose a deck, an opponent, and the available rule mode.

A **free table** lets players manage play themselves. In **rules-enforced**
mode, the game plugin accepts or refuses actions. The plugin may have
limitations; a refusal is not necessarily an official ruling.

The deck editor is also available from the home screen. Create a deck or
import one in a supported format, check the reported problems, and save it.
Saving a deck and choosing it for a table are separate actions. A list of
card names alone does not provide the card images.

## Play with another person

One person hosts; the other joins with the host's invitation. A nearby or
known table may also appear in the join list. An invitation provides connection
information: share it only with the people you want at your table.

Use matching application builds when troubleshooting a failed join. If Kai
reports a wire-protocol mismatch, both players must update to compatible
builds. The host's game modules are fixed for that table; changing a local
plugin does not update an ongoing match.

A host coordinates the game. This is not a system for playing against a
malicious host, and reconnecting is not a guarantee against losing a match.

## Interact with cards

Select a card to see its available actions. Dragging requests a move; in
rules-enforced mode the game may refuse it. Follow the active prompt before
trying another action. Some prompts concern cards not currently on the table.

Use the table's help panel for the hotkeys available in your build. The table
menu gives access to help, history, and other table tools. Settings control
presentation and input; they do not change the game rules.

On a small screen, some controls move into drawers or sheets. Close the topmost
sheet before expecting Back or Escape to leave the table.

## If something looks wrong

Missing art and an invalid deck are different problems. An image may still be
loading even when the deck is usable. A legality warning can also mean that
the available card metadata is incomplete.

Before reporting a problem, record the version in Settings, the selected game,
rule mode, device, and steps that reproduce it. Do not post other players'
private hands or unredacted session data.

Automated opponents are optional on native builds. Model-backed opponents may
send game context to an external model provider; do not enable them with
information you do not want that provider to receive.
