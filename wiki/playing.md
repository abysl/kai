# Playing on a Kai table

Audience: players with an installed or running copy of Kai. No programming
knowledge is needed. For installation from source, use the
[development guide](development.md).

## Track your personal Elo

Open **Settings → You → personal Elo**. This is an honor-based personal estimate,
starting at **1200**, saved only on this device (or this browser's site storage).
After a game, enter your opponent's self-reported **pre-game** Elo, select your
own **win**, **loss**, or **draw**, check the preview, then press **record result
locally**. Record games in order: the current estimate is your pre-game rating.
Each player records independently; playing P2P does not require recording results.

The estimate uses ordinary Elo with **K=32**, rounding each change to the nearest
whole point, with halves away from zero. An equal-rated win adds 16; a loss
subtracts 16; a draw changes nothing. Enter whole-number opponent ratings between
-10000 and 10000. One estimate is shared across games in this installation.

**Recent results** explains the latest 50 changes. **Undo latest result** restores
the previous estimate and fills the form so you can correct and re-enter it.
For an older retained mistake, undo back to it, then re-enter the corrected result
and later results in order. Older entries outside the 50-result window cannot be
undone. Use one running instance or browser tab; estimates do not sync between
devices. Clearing app/site data removes the estimate and history.

This feature is a personal record, with no verified ranking or leaderboard.
See the [calculation and storage details](design/personal-elo.md).

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
