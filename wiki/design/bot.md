# The Bot — Auto-Pass, Forced Answers and `hold`

The LLM seat used to be woken for every pass. `actionable` — the labels of
every enabled, shown, non-reveal affordance — is the decision key `think`
gates on, so a lone `pass`, a lone `end turn` and a two-card discard were each
a "new situation" and each cost a model round. Half a Riftbound game is
priority windows in which the seat has nothing to do; the model was paying
for all of them, and a player watching the chat saw "ai deciding…" a dozen
times a turn. This page describes the pilot that now sits between the table
and the brain: one judgement of whether a view offers the seat anything
beyond a pass, shared with the desktop's automation; the forced moves the
driver makes on its own; and the `hold` tool, which lets the model sleep
through a stretch on purpose and be woken with a note of what happened.

The desktop's human automation ([table.md — Automation](table.md#automation),
[ux.md §4.6](ux.md#46-automation--client-side-over-the-existing-pass-affordance))
is unchanged: `auto::decide` still never presses end turn for a player, still
waits on every prompt with a real choice, and the soak's parity test still
shows the auto-passing seat playing the manual seat's game on the same seed.
What changed is that its "is there anything to do" question is now one
function both sides call.

## The one judgement: `auto::offer`

`auto::offer(view, me) -> Offer` (`src/table/auto.rs`) reads a `PluginView`
and says what the seat is offered:

| `Offer`          | when                                                                                                                                                                     |
|------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `Theirs`         | a prompt is open for another seat and I have neither a legal row nor a roll to send                                                                                       |
| `Forced(index)`  | a prompt for me whose enabled options are exactly the `min == max` remaining card picks with no cancel, no optional skip and no non-card option (the desktop's auto-answer shape, `forced_index`); or a lone roll to commit (`lone_roll`: every enabled non-panic non-reveal affordance is a `Commit` and nothing is legal) — behind another seat's prompt, on my own shuffle prompt, or with no prompt at all |
| `Choice`         | a prompt for me that needs a real choice (`open_shape`: optional, `min != max`, `max == 0`, or a cancel on offer), or a pass / end turn primary with a move beside it (`has_move`), or moves with no primary at all |
| `Pass(index)`    | the pass primary (`primary::pass_index`, hotkey `w`) and no move                                                                                                          |
| `EndTurn(index)` | the end-turn primary (`primary::end_turn_index`, hotkey `space`) and no move                                                                                              |
| `Nothing`        | no prompt, no primary, no move — the seat is waiting on the other side                                                                                                    |

`has_move` is the existing rule: any `view.legal` row with a non-empty
`kinds` (Play, March, Activate, React, Answer, Hide) or any shown, enabled
affordance other than the primary, the free-table offers
(`hud::is_free_table_offer`) and reveals. `Offer::is_quiet()` is
`Pass | EndTurn | Forced` — the presses that need nobody's judgement.

`decide` is expressed over `offer`: a prompt for another seat is
`Wait(Theirs)` before `offer` is consulted, so the desktop never sends the
roll behind an opponent's prompt on the player's behalf; a prompt for me goes
through `answer`, which keeps `ask_anyway`, `order_triggers` and
`assign_damage` in front of the `Forced` shape and never presses a `Commit`
for the player (`auto::is_commit`: the roll on the player's own shuffle
prompt is `Forced` for the bot and `Wait(Choice)` for the player, who still
clicks it); with no prompt the pass is the only primary `decide` presses,
and it fires exactly on `Offer::Pass` — `Offer::EndTurn` stays
`Wait(NoPass)` for a player. The test
`the_offer_is_the_one_judgement_of_whether_the_seat_has_anything_beyond_a_pass`
walks every shape and asserts the player's auto-pilot fires on exactly the
pass and the forced pick the offer names.

`auto::manual_kind` is honoured to the click on the desktop: with "order my
triggers myself" on, the player answers the last pick of an order too. The
bot's exemption for the last pick (`remaining(summary) > 1` in
`hold::breaks`) is the bot's own — a pick with one result is not a decision
worth a model round — and is a deliberate difference, not an oversight.

`driver::auto_choice` (the `Mind::Auto` seat and the soak's auto seat) is
`offer` mapped to a `Choice::Action`: `Pass`/`EndTurn` as a pass, `Forced` as
an answer, the rest `None`.

## The pilot: `ai::hold::Pilot`

`Pilot::step(view, me, board_cards, fresh_chat) -> Step` is what the LLM
driver runs every tick before it thinks:

- **`Step::Idle`** — `Offer::Nothing | Theirs`: the table is with the other
  seat; nothing is sent, nothing is asked — and a hold whose condition has
  just held keeps standing (its note keeps growing) until a view the seat
  can act on, so the wake spends the note on a real decision, not on a
  "waiting for …" view with nothing to press.
- **`Step::Press { index, label, pressed }`** — `Offer::Pass`, `EndTurn`, a
  `Forced` pick and a lone roll are sent at once, without the brain. The
  bot presses end turn where the player's automation would not: a turn with
  nothing legal and nothing to activate is over. A forced pick is not
  pressed when it is a trigger order or a damage assignment with more than
  one pick left (`BOT_PREFS` has `order_triggers` and `assign_damage` on,
  `auto::manual_kind`): those go to the model, the last pick of one does
  not.
- **`Step::Model { held }`** — anything else wakes the brain: a prompt that
  needs a real choice, the bot's own turn with plays open, a reaction on
  offer when no hold is set, a chat line from the player (`fresh_chat`), or
  a hold whose condition has just held. `held` is empty unless a hold ended.
- **A free table is the model's alone.** `plugin_ui::enforced(view)` is
  checked before any press: on a free table nothing is forced — a lone
  `end turn` means the model has not drawn, played or attacked yet, not
  that the turn is empty — so every tick is `Step::Model` (with `think`'s
  key gate deciding whether the model is actually called, as before the
  pilot), `Pilot::hold` refuses with `FREE_TABLE`, and a hold that was
  standing when the table went free ends with "because the table went
  free".

The driver keeps its `Sent`/`FOLD_WAIT` pacing for the pilot's presses: one
press per tick, and the next only after the host folded (or refused) the
last, so the seat never runs ahead of its own replica and the halt flag is
checked between every press. Presses are counted, not logged; the first
`Step::Model` after a quiet stretch prints one line — "passed 7 times with
nothing to do", "passed twice and answered 1 forced prompt with nothing to
decide" (`Pilot::stretch`).

`decision_key` is unchanged and `think` still refuses the same seq and
label set twice — except when it is handed a hold's note (`held` non-empty),
which is a wake in its own right like a chat line, and after a refused hold
(`last_decision` is cleared so the model is asked again with the refusal in
front of it); the pilot decides whether `think` is called and whether it is
a wake.

## `hold`

`hold` is a brain tool (`tools()`, `HOLD_TOOL` in `hold.rs`, offered once the
deck is dealt — `IN_GAME_TOOLS`). Its arguments are `until` (one of
`CONDITION_WORDS`), `card` for `card_named`, `count` for `passes`, and
`any_of` for `any_of`; a list in `until` is `any_of` without the word.
`Condition::parse` refuses what it cannot read with the reason in the tool
result ("hold refused: card_named needs card…") and the round goes on;
an accepted hold ends the decision like `done` does and is recorded on the
`Brain` (`take_hold`/`hold_requested`) for the driver to pick up after
`decide_until` returns.

| condition         | holds when (`Hold::check`)                                                                                                                        |
|-------------------|---------------------------------------------------------------------------------------------------------------------------------------------------|
| `my_turn`         | the turn line names me and its `(number, seat)` differs from the one the hold was set on                                                          |
| `playable_action` | `auto::offer` is `Choice`                                                                                                                         |
| `opponent_played` | a chain row with `seat != me` and an item id not seen when the hold was set (`Hold::observe`) — a spell, a trigger or a battlefield ability alike, since a `ChainRow` carries no origin; the tool text and the wake say "puts something on the chain" |
| `showdown`        | a `showdown at …`/`combat at …` status line or an `Attack`/`Combat` arrow appears where there was none                                            |
| `card_named(n)`   | a card named `n` (case-insensitive) is on the board — a `ZoneKind::Battlefield` zone every seat sees, so a base or a battlefield, not the trash, banishment, rune pool or chain (`Seat::board_cards`) — with an id not there when the hold was set |
| `passes(n)`       | the pilot has sent `n` passes since the hold was set                                                                                              |
| `any_of[...]`     | any one of the parts                                                                                                                              |

While a hold stands, the pilot passes through what it would otherwise wake
the model for — a reaction on offer is passed, not asked — and keeps making
the forced moves. It still breaks on anything only the model can answer: a
prompt with a real choice, a trigger order or damage assignment that
matters, the bot's own turn with plays open, a legal row behind another
seat's prompt ("you have something to play"), and a chat line from the
player. The wake reason is the first line of the note.

A hold is refused at registration when it would break at once — a decision
is open now (`hold::breaks`: "a question only you can answer: …", "your
turn has plays open") or its condition already holds — or the table is
free. `Pilot::hold` returns `Dropped { reason, again }`: the driver logs
`ai hold dropped: …`, and `driver::hold_dropped` tells the model in the two
places it reads next time — a notice in the table state ("your hold until …
was dropped: …", `Seat.notices`) and the recap's "You finished with:" line
(`Brain::hold_dropped` rewrites `recap.reason`, which the `hold` tool result
had already set to "held until …") — then clears `last_decision` so the
model is asked again at once with the refusal in front of it. `again` is
set when the same view was refused before: the model has been told once,
so the driver presses the idle `fallback` instead (`nudge`) and the table
moves on. Without that, a refusal on the bot's own turn ended the turn for
it behind the model's back. The soak does the same through
`driver::hold_dropped` and `Table::nudge`.

**The note.** `Hold::observe` runs each tick and records what the model will
want to know: every chain row another seat put up ("{seat 1} put {card 40}
on the chain"), the narration lines that arrived (the sliding window's new
tail, `new_lines`), a fight that opened, the forced answers the pilot gave
("you answered \"discard a card\" with {card 5}", "you sent your roll").
`Hold::report` prefixes "You held until …; the hold ended because …" and
closes with the pass and answer counts. The driver expands the placeholders
with `Seat::expand_notice` and hands the lines to `think` as
`Situation.held`; `Brain::user_prompt` prints them under
`## What happened while you held` before the table state, and the system
prompt tells the model it is not asked about every pass, what wakes it, and
that a reaction does not wake a `my_turn` hold.

The `hold` tool's schema offers `LEAF_WORDS` (the six conditions) for the
`any_of` items and `CONDITION_WORDS` (those plus `any_of`) for `until`, so a
nested `any_of` — which `Condition::parse` refuses — is unrepresentable; the
`card` and `count` properties say which word they belong to.

The random mind ignores holds — `roll` never consults the pilot. The auto
mind sends the same presses the pilot would (`auto_choice`), and nothing
issues holds on its behalf.

## Where it runs

The in-process seat (`ai::local`, the `kai-ai-seat` thread), `kai-cli
--brain random|auto|nanogpt` and `kai-cli --ai` all run `Driver::tick`,
which for an LLM mind calls `Driver::steer`: chat, then the pilot, then
`think` on `Step::Model`. The soak (`kai-cli soak`, `Runner::step`) keeps
one `Pilot` per seat and runs the same `step` before `Table::think`, tracing
the presses as `auto-sends`, the stretch lines, `holds until …`, `hold
dropped: …` and the `held:` note lines. `Table` counts `quiet_steps`,
`brain_calls`, `brain_calls_while_quiet`, `holds` and `held_wakes`;
`GameRecord` carries `quiet` and `brain_calls`. Quiet is
`hold::is_quiet(view, me)` — rules enforced, `offer.is_quiet()` and
`breaks` none — so the counters measure exactly what the pilot would have
sent, not the `Forced` trigger orders it hands to the model.

## Tests, without a model

- `auto.rs` — `the_offer_is_the_one_judgement…` (every `Offer` shape, the
  lone roll behind another seat's prompt, and the parity with `decide`'s
  firings); the existing `decide` tables are unchanged.
- `hold.rs` — `Condition::parse` and `describe`; the pilot's quiet stretches
  and their one-line report; trigger orders and damage assignments that
  matter go to the model and the last pick does not; a hold passing through
  a reaction and waking on my turn with the note; every condition judged over
  the view and a question breaking any hold (a met `card_named` stays idle
  on a waiting view and wakes on the next actionable one); a hold refused
  while a decision is open (the second refusal on the same view is `again`,
  a legal row behind the opponent's prompt is "you have something to play")
  and a lone roll sent without the model; a free table is the model's alone
  and holds nothing; `is_quiet` is what the pilot would send.
- `brain.rs` — `a_hold_call_ends_the_decision_with_its_condition_and_a_bad_one_is_refused_in_the_round`
  over `decide_with` with a scripted chat, `hold_dropped` rewriting the
  recap, and the `any_of` items enum being `LEAF_WORDS`; `hold` in
  `every_tool_has_a_name_and_a_schema` and hidden before the deal in the
  tool-set test.
- `nanogpt.rs` — `Client::canned(model, replies)` answers `chat` from a
  queue shared by clones and never touches the network
  (`CANNED_EXHAUSTED` once empty).
- `kai_cli/soak.rs` — `a_canned_llm_seat_is_woken_only_for_real_choices_and_its_holds_end_with_a_note`
  plays a capped game with a scripted brain (`Player.script`) that alternates
  `hold any_of[my_turn, opponent_played]` and `done`: no brain call lands
  while the seat is quiet, the pilot sends the quiet presses, at least one
  hold is set and at least one ends with the note. The two random soak tests
  assert `record.quiet > 0 && record.brain_calls == 0`.

The NanoGPT account has no balance; nothing here makes a live call.
