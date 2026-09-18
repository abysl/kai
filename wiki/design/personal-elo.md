# Personal Elo

Kai keeps an honor-based personal estimate for the person using this local
installation. Settings → You provides manual opponent-rating and result entry,
a preview, and the latest 50 results. Recording is independent of P2P gameplay:
players can record at different times, including after disconnecting, and either
player can choose not to record. There is one estimate per local store, shared
across games, with no player identity attached.

## Calculation

The initial rating is 1200 and the fixed K factor is 32. For the player's
pre-game estimate `R`, opponent's self-reported pre-game rating `O`, and score
`S` (win 1, draw 0.5, loss 0):

```text
expected = 1 / (1 + 10^((O - R) / 400))
change = round(32 * (S - expected))
new estimate = R + change
```

Round the change to the nearest integer, with exact halves away from zero.
Store integer ratings; there is no rating floor or pairwise balancing. Opponent
entry accepts trimmed signed decimal integers from -10000 through 10000.
Fractions, scientific notation, non-finite values, and out-of-range inputs are
rejected. Negative estimates are allowed by ordinary Elo. The broad input bound
catches entry mistakes without restricting normal ratings. Entry uses the current
estimate as the pre-game rating, so record games in order and enter the opponent's
rating from before that game.

From 1200, wins/losses/draws against 1200 produce 1216/1184/1200; against 1600,
1229/1197/1213; against 800, 1203/1171/1187.

## Local history and correction

`src/elo/` owns calculation, storage, and the small settings panel. The settings
plugin loads the history at startup. Each entry retains its before/after ratings,
opponent rating, and result. Evicting the oldest entry advances a baseline so
undoing the retained entries never resets older progress. Undo removes the latest
entry and restores its exact before rating; it also fills the form for correction.
Correct an older retained entry by undoing back to it and re-entering it and later
results in order. Entries outside the retained window cannot be undone.

Native builds use `personal-elo.json` in `os::paths::config_dir()` (desktop XDG
configuration or Android app storage); browsers use `kai.personal-elo` in the
current origin's localStorage. JSON contains a format version, baseline, and
bounded entries. Loading checks format and calculation consistency to detect
accidental corruption. This is local data validation, not result verification.

A native save writes a temporary file beside the destination and renames it.
A browser save replaces the localStorage item. Memory changes only after a
successful save, including undo. Read or decode failures leave the stored data
untouched and disable recording until a successful retry. A missing store starts
at 1200. There is no cloud backup; clearing app/site data loses the estimate.
Use one running Kai instance or browser tab per local store. Concurrent instances
are not coordinated; the latest save wins. Native saves are atomic replacements,
not a power-loss durability guarantee.

## Scope

No Elo networking, remote agreement, accounts, identities, signed receipts,
central storage, ranking API, leaderboard, matchmaking changes, replay checks,
anti-cheat, or authoritative hosting are part of this feature. Existing Agni
sessions and wire formats are unchanged. A future competitive system would be
a separate design; this implementation contains no scaffolding for one.
