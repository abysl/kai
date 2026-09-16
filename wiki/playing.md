# Playing on a Kai table

Audience: players with an installed or running copy of Kai. No programming
knowledge is needed. For installation from source, use the
[development guide](development.md).

## Undo a mistake

At a hosted table, press **Shift+Backspace** or **Ctrl+Z** once for each action
you want to undo. Kai waits one second after your last press, then sends one
request for the total. You can cancel the queued request before it is sent.
`R` still reveals a card; typing in a text field does not request a table undo.

Every other seated player sees an **Agree / Decline** prompt. The table only
rolls back after everyone agrees. If somebody declines, disconnects, or makes
another game action, the request is cancelled. Local automated opponents agree
automatically. A host playing alone needs no vote.

Undo affects the last actions at the whole table, including your opponent's,
not just your own actions. A card play and its automatic reveal count as one
action. Up to 64 actions are kept; joining, dealing/reloading a deck, and starting
a new game clear that history. Undo restores game state, but cannot make someone
forget a card they already saw. All players need a compatible version of Kai.
The offline free-form sample table has no session history and does not support undo.

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

## Add an AI player

AI players are available on desktop, Android, and in the browser. You must
host the table to add one; players joining somebody else's table cannot do so.

1. Choose your game and deck in the lobby. Under **opponent**, choose **AI**.
2. Use **switch decks** on the AI's deck card if you want a particular matchup.
   In the browser, **let the AI pick** selects a sample Riftbound deck locally.
3. Press **play vs AI**. The **AI player settings** screen opens before the
   table is created. At an existing table, **add AI** opens the same screen.
4. Choose **OpenRouter** or **NanoGPT**, and paste your API key for that provider.
5. Search the model list and select a model from the dropdown. Only models
   advertising tool support are shown, because the AI uses tools to make moves.
6. Press **Add AI player**. You can use **AI settings** in the lobby to prepare
   different settings for the next AI player, or **stop AI** to stop the current one.

You can instead select **Random player** for an opponent that needs no key and
makes no paid model requests. Closing the settings screen without confirming
does not create a table or add a player.

Keys stay in memory for the current app session or browser tab. They are not
saved to disk, included in invitations, or sent to Kai's server. Reopening Kai
requires entering the key again. A native client can also read the selected
provider's `OPENROUTER_API_KEY` or `NANOGPT_API_KEY` environment variable.

Model-backed play sends the AI's visible table state, card information and
your AI chat messages directly to the selected provider. That provider's
privacy policy and API charges apply. Use a key with a spending limit where
available. Loading the model list does not verify that your key has credit or
access to a particular model. Authentication, credit and connection errors
appear in the AI status; stop the AI and check its settings before trying again.

Keep a browser-hosted table's tab open and visible. Browsers may throttle hidden
tabs; closing or reloading the tab stops its AI player and discards its key.

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

## Mulligans and recycling

Choose which cards to mulligan or recycle as before. When those cards need to
be shuffled, each player's app contributes to the shuffle automatically; there
is no extra **roll** click, including for the other player's mulligan. This
does not choose cards for you or pass a response window. The opening roll for
first player remains manual. Both players should update for automatic shuffling.

## Order simultaneous triggers

When several triggers happen together, Kai asks you to choose their order,
even with automatic forced answers enabled. Choose the trigger that should
resolve last first: the last one placed on the chain resolves first.

For Dusk Rose Lab and a Temporary unit, choose **Temporary first**, then
**Dusk Rose Lab**. With only one trigger left, Kai may place it automatically.
The Lab resolves before Temporary and lets you sacrifice the unit for a card
while it is still there.

## Azir and empowered units

Azir remembers Equipment you played earlier in the turn, even if it has since
left play. His ability still needs one energy, a ready legend, and a legal
activation window. A gear without Equip does not meet the condition.
The Sand Soldier's Weaponmaster choice appears when the token is played;
domain-specific Equip costs still need to be paid.

After an Empower ability resolves, the Might badge and inspector show the
rule-calculated value. Steel Paws goes from 0 to 7 without an additional manual
counter change.

Token images load at runtime. When the content catalog lacks a token image,
Kai fetches it from Piltover Archive's public image CDN. No card artwork is
bundled in this source release; an internet connection is needed on first load.

## If something looks wrong

Missing art and an invalid deck are different problems. An image may still be
loading even when the deck is usable. A legality warning can also mean that
the available card metadata is incomplete.

Before reporting a problem, record the version in Settings, the selected game,
rule mode, device, and steps that reproduce it. Do not post other players'
private hands or unredacted session data.

Do not include API keys in bug reports or screenshots. Model-backed opponents
send game context to an external provider; do not enable them with information
you do not want that provider to receive.
