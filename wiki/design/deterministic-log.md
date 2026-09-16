# Deterministic Action Log — From Host-Authoritative to Log-Authoritative

> Status: phase A implemented (see the phase table and the implementation
> notes under it); phases B–D remain design. Answers the question "instead of
> host authoritative can we
> do a deterministic append only log of game actions or crdt to
> deterministically resolve state on client side." Short version: yes to the
> log, mostly no to the CRDT, and the hard part is neither — it's hidden
> state. Builds on [multiplayer.md](multiplayer.md) (the current protocol),
> agni's [architecture.md](../../../agni/wiki/design/architecture.md)
> (determinism as the load-bearing property; this doc is the Phase-6 "input
> exchange and ordering" design that multiplayer shipped ahead of), and
> spirit's [identity.md](../../../../spirit/spirit/wiki/design/identity.md),
> [attestations.md](../../../../spirit/spirit/core/wiki/design/attestations.md)
> and [terminology.md](../../../../spirit/spirit/wiki/design/terminology.md)
> (deterministic CBOR, signed records, persisted node keys — primitives this
> design uses rather than reinvents).

## Where today's protocol actually stands

The current session (`agni/net/src/host.rs`, re-exported as `agni_net::session`) is closer to a log than the
"host-authoritative" label suggests. The host already numbers every state
change (`HostSession::next_seq`), clients already fold events into a replica
(`ClientSession::apply` is client-side resolution, full stop), and events
already carry intent-shaped payloads (`Dealt`, `Moved`, `Reset`). What makes
it host-*authoritative* rather than log-authoritative is three things:

1. **Snapshots are the source of truth at the seams.** `Welcome` and `Reset`
   ship a full card snapshot; the event stream is a delta feed over it. A
   masking or replay bug produces replicas that silently disagree and never
   reconcile — the divergence class agni's architecture doc calls "close to
   undebuggable after the fact."
2. **The host mutates state through a different code path than clients
   replay.** `HostSession::deal` calls `Table::add_face`; clients fold
   `insert_card` from wire cards. Two paths, one invariant, enforced by hope.
3. **The log is not an artifact.** Events are consumed and dropped. Nothing
   can be replayed, re-hosted from, attached to a bug report, or handed to a
   late joiner. When the host dies, the state dies with it — the documented
   `session ended — table frozen` limitation, made worse by phones being
   fragile hosts.

The refactor is therefore not a rewrite. It is a change of authority: **state
becomes a pure function of an ordered log of actions** — `state =
fold(genesis, log)` — and the host's remaining job is to *order* the log, not
to own the state.

## The design in one paragraph

One append-only log per table. Entries are actions (join, deal, move,
reveal), canonically encoded as deterministic CBOR (RFC 8949 §4.2, per
terminology.md — the encoding spirit already decided), hash-chained
(`prev = blake3(canonical(previous entry))`), and — from phase B — signed by
the acting node's persisted key (identity.md phase 2). Every peer, host
included, computes state by the same fold over the same bytes. The host is
demoted to **sequencer**: it assigns `seq`, appends, and broadcasts; it
validates intents before appending, but the fold re-validates
deterministically, so even a buggy or hostile sequencer cannot make replicas
diverge — an invalid sequenced entry is a deterministic no-op everywhere.
Hidden faces never enter the shared log at all: the log carries card ids,
counts and (eventually) commitments; faces ride a private side channel to
their owner and enter the log only on reveal. Any peer holding the log can
re-host.

## Ordering: why the sequencer stays and full CRDT is rejected

A CRDT gives you convergence without coordination by requiring concurrent
operations to commute. Look at the actual operation set before paying for
that:

| Operation | Commutes with concurrent ops? |
|---|---|
| Move within/into a board, at an index | **No.** Two inserts at index 2 are order-dependent; board order is a sequence, and sequences need RGA/fractional-index machinery to become CRDTs |
| Draw from a (future) shared deck | **No.** Two concurrent draws from one deck top is a conflict by definition |
| Move within my own hand | Yes — nobody else's ops touch my hand |
| Reveal | Yes — grow-only, idempotent |

The non-commuting cases are the ones that matter, and card games serialize
them *by their own rules*: turns, priority, "one thing resolves at a time."
The concurrency window a CRDT would arbitrate is the few hundred milliseconds
between two friends dropping cards at once — and the game this engine renders
today is a shared physical-style table where board cards are anyone's to
move and a simultaneous grab resolves exactly like it does at a real table:
someone's hand gets there first and nobody files a complaint. **A 200ms
reorder is nobody's problem in this game.** Buying RGA-style sequence CRDTs
per zone, plus a shared-deck conflict story, to shave the failover gap is
engineering vanity. Full CRDT: rejected.

The realistic options for ordering:

- **(a) Keep a sequencer, make it replaceable.** Total order by construction,
  zero reordering, zero rollback. Failover = election + resume from the log
  (see below). This is the choice.
- **(b) Lamport clocks + deterministic tiebreak + rollback.** Workable once
  state is a pure fold (rollback is just re-fold from an earlier point, which
  the log makes cheap), but it trades an invisible failover stall for
  *visible* mid-game rollback — a card teleporting back off the board is
  strictly worse UX than one second of "reconnecting." Kept in the back
  pocket; not built.
- **(c) Per-zone commutativity.** Moves inside your own zones could bypass
  the sequencer entirely and merge by actor. True, cute, and an optimization
  of a problem (sequencer round-trip latency over LAN/iroh direct paths,
  single-digit ms) that does not exist. Not built.

**Election** reuses the rule spirit's gossip layer already uses for backfill
(gossip.md): *lowest node id among currently reachable peers wins*. Node ids
are stable public keys once identity.md phase 2 persists them, so every
peer computes the same winner from the same membership with no coordination
protocol. On sequencer loss: freeze intents, elect, candidates exchange
`(last_seq, last_hash)`, the longest valid signed chain wins (ties cannot
occur under a single live sequencer; a partition heals by longest-chain, and
the losing tail — at most a few in-flight moves — is re-proposed as fresh
intents). This is thinkable *only* because any client holding the log can
reconstruct the exact table; it is the payoff of the whole refactor.

## Hidden state is the real problem — name it honestly

A deterministic shared log cannot contain "the contents of my hand" in
plaintext: the log is by definition the thing everyone replicates, hashes and
replays. And "resolve everything client-side" collides head-on with shuffles
and draws, which must be *unpredictable to you* and *private to me*. Every
log-based design has to pick an answer here, and most of the exotic ones are
answers to questions this game is not asking.

The survey, honestly:

| Approach | What it buys | What it costs | Verdict |
|---|---|---|---|
| **Mental poker** (commutative encryption, full trustless shuffle) | Nobody, including the dealer, knows hidden cards | Heavy crypto, per-game protocol, brutal complexity | Overkill. Rejected outright. |
| **Per-player encrypted payloads in the shared log** | One log artifact, faces recoverable by owner from the log alone | Log entries stop being canonical-for-everyone unless ciphertext is part of the canonical bytes; key management; the log leaks size of payloads anyway | Not needed — see below |
| **Committed shuffles / commitments in the log** | Reveals become *verifiable*: an opened face must match its commitment, so an owner cannot swap cards after the fact; removes the trusted dealer | Requires owner-side dealing and (for deck games) committed decklists to mean anything | Right shape, wrong time — phase D |
| **Hidden zones stay per-owner state; the shared log carries ids + counts, faces enter on reveal** | One canonical log, byte-identical for every peer, hash-chainable, re-hostable; matches the current trust model exactly | Reveals are trusted, not proven (same as today, where the dealer sees everything anyway) | **The design, phases A–C** |

What the current masking already gets right, and what any redesign must
preserve as invariants:

- Opponents learn your hand **size** and your **reveals**, never contents
  (`mask_cards` strips faces; the renderer never even spawns hidden-card
  entities).
- Once public, always public — a revealed card stays revealed, like paper.
- You cannot move a card out of, or into, a hand that is not yours
  (`HostSession::intent` refuses both).

The log-based restatement is *stronger* than today's masking, not weaker:
today the host masks per-recipient, so each seat receives a different byte
stream — which is precisely why the event stream cannot be one shared,
hashable artifact. Instead, define the canonical log to contain **no hidden
faces, for anyone, ever** — a `Dealt` entry carries card ids, owner, zone and
count only. The owner's faces travel in a private companion message on their
existing per-client QUIC stream (and are, of course, already in the owner's
own store once deals are identity refs). `Reveal` puts a face into the log
when a card goes public, exactly where the current `Moved { face: Some(..) }`
reveal lives. Consequences worth spelling out:

- The log is **byte-identical at every seat** — one hash chain, one artifact.
  Late join, re-host, desync detection and replay all key off this.
- The host's *own* hand is also absent from the shared log. The sequencer
  holds no privileged state; that is what makes it replaceable.
- On re-host, un-revealed faces are not reconstructible from the log — and do
  not need to be: each owner still holds their own faces and simply keeps
  playing. A player who disconnects *and* loses local state loses their
  un-revealed hand; so does a player at a physical table who drops their
  cards in a river. Acceptable.
- **Who deals?** Today the host deals every hand from its own store (which is
  also why two players can hold the same card — there is no shared deck yet).
  In a log-first world the natural endpoint is *owner-deals*: each player
  appends their own `Dealt` entry (ids + count, faces stay home) drawn from
  their own store. Without commitments this is trusted — your friends believe
  your reveals — which is exactly today's trust level, minus the host reading
  everyone's hand, so it is a strict improvement. *Verifiable* owner-dealing
  (commitments, and committed decklists once decks exist) is phase D.

## The log rides spirit's primitives

Nothing below is a new mechanism; every piece is already designed or
implemented in spirit, per the identity.md rule that card identity work must
not fork the general model.

**Entry format.** A log entry is a record in the terminology.md sense:
canonical deterministic CBOR, hashed with blake3.

```toml
[entry]
table  = "log:<genesis-hash>"     # which log this extends
seq    = 42
prev   = "blake3:<hash of entry 41's canonical bytes>"
actor  = "nodeid:<ed25519 pubkey>"
seat   = 2
action = { kind = "move", card = 17, to = "board", seat = 1, index = 0 }

[proof]
sig = "base64:..."   # actor's node key over blake3(canonical(entry))
```

The proof block is excluded from the signing scope, the same load-bearing
detail attestations.md pins — phase A entries can go unsigned and phase B
signs the identical bytes, no re-minting. Actions cover what `TableEvent`
covers today plus what the log newly needs: `genesis` (table config, initial
seats), `join`/`part` (the roster must be *in* the log, or a re-host cannot
reconstruct seating), `deal` (ids + owner + count, no faces), `move`,
`reveal` (face or, later, `(card ci, printing ci)` + opened commitment),
`reset` (a new shuffle/deal cycle as an *entry*, not a snapshot — the
snapshot-bearing `Reset` dies with the refactor).

**Keys.** Entry signatures use the persisted iroh node key — identity.md
phase 2's first task, needed there for attestations and here for the log;
one keypair, no new crypto. Seat↔node binding is established by the signed
`join` entry, so "you may not move cards out of another's hand" becomes "the
fold rejects a hand-move whose entry signature does not match the hand
owner's bound key" — enforced identically at every replica instead of only
at the host.

**Card identity.** Entries reference cards by table-instance id plus, once
identity.md phase 3 lands, `(card ci, printing ci)`. This is what finally
kills the JPEG-bytes-in-hands wart from multiplayer.md's known limitations:
the log carries *identity*, every peer resolves art from its own replicated
store, and a 7-card deal is a few hundred bytes instead of a few hundred KB.
The log design does not depend on phase 3 (faces can ride the private
companion message meanwhile) but sequencing them together avoids designing
the `deal`/`reveal` payloads twice.

**Transport and replication.** Live play keeps the existing `spirit-table/0`
ordered QUIC streams — gossip's pull-based, seconds-scale convergence is the
wrong tool for gameplay latency, and the one-protocol-per-ALPN rule says the
table protocol owns its own channel. What changes is the payload semantics:
the host streams *log entries*; `Welcome` becomes seat assignment plus the
log prefix (or, as a pure optimization per agni's architecture doc, a
snapshot *plus the seq and entry-hash it folds to*, verifiable against the
chain — snapshots may accelerate, never define). A finished or abandoned
game's log is one blob: content-addressed, advertised as a ref, replicated
by the existing mesh machinery for free — which is replayable games, bug
reports that reproduce, and spectating-after-the-fact with zero new
protocol. Periodic state-hash entries (agni's open question) give cheap
desync detection and bound divergence hunts.

## Migration from session.rs, phased

What survives: almost everything structurally. `TableEvent` is two-thirds of
the action vocabulary; `ClientSession::apply` is the seed of the fold;
`mask_*` logic becomes the deal/reveal split; seat/roster handling moves into
`join` entries; the intent-validation body of `HostSession::intent` becomes
the fold's validation. What dies: snapshot-as-truth (`Welcome`/`Reset`
payloads), the host-only mutation path, per-recipient masked event streams.

**Phase A — state = fold(log); host stays sequencer. SHIPPED.** Introduce the
entry type (unsigned), make the host append-then-fold through the *same* fold
clients run, carry the log in memory, replace `Reset`'s snapshot with a
reset entry, and add the fold-equivalence determinism tests (fold(log) ==
incremental state, plus a replayed-corpus test per agni's architecture).
Hidden faces move to the private companion message so the log is one
artifact. This alone kills the replica-divergence bug class, makes every game
replayable, gives bug reports a reproducer, and settles agni's Phase-2
"replay format" open question with running code.

Implementation notes, where the shipped code deviates from the paragraph
above (all other decisions landed as designed):

- `Welcome` went straight to seat + roster + **log prefix** instead of the
  interim stamped snapshot — that item was pulled forward from phase B during
  scope review, because the log is small without faces, the joiner then
  exercises the same fold as everyone else from entry zero, and the
  `(seq, hash)` stamp is moot when no snapshot ships. Hash chaining itself
  (populating `prev`) remains phase B; snapshot-as-optimization can return
  later under the "accelerate, never define" rule if logs ever get long.
- The fold lives in `agni/sim/src/log.rs` (`LogEntry`, `LogAction`, `LogState`,
  `fold_entry`/`fold`, `validate`). Entries carry the full wire shape —
  `table`, `prev`, `actor` ride as `None` until phase B fills them, the proof
  block stays absent (it is outside the signing scope anyway) — so B signs
  without re-minting. `HostSession` is now the sequencer: every append is
  validated and folded through the client fold, there is no host-only
  mutation path left.
- A reveal is its own entry, sequenced immediately before the move that
  makes the card public; `Moved { face: Some(..) }` is gone. The sequencer
  supplies the face from its dealer store (it still deals every hand in
  phase A, so this preserves today's trust level exactly; owner-deals is
  phase B). Revealed faces survive a return to hand — "once public, always
  public" now holds for late joiners too, where the old per-recipient
  snapshot masking silently re-hid them.
- Per-recipient masking is dead: the log is byte-identical at every seat
  (tested by encoding it at three replicas), deals carry ids/owner/count
  only, and each owner's faces arrive via a `Faces` companion message on
  their existing per-client stream. Every replica — host included — renders
  fold-state plus only its *own* face overlay.
- An invalid sequenced entry consumes its seq slot as a deterministic no-op
  at every replica (tested per rejection reason); the sequencer pre-validates
  with the same `validate` the fold uses, so it never appends one.
- `genesis` carries only the host seat today — kai has no table config
  (ref/set, per-owner deal counts) to capture yet; the variant grows fields
  when the config exists. `part` entries and connection liveness stayed out
  of the log for now: `Roster` messages keep carrying connected flags, and
  the roster's log-worthy content (seat, name) is in `genesis`/`join`
  entries. `part`/rejoin belongs with phase B's key-bound seats.
- Opening a table imports the pre-session solo table *through the log* —
  genesis, one deal, then reveal+move pairs for board cards — so
  state = fold(log) holds from the first frame and the host keeps its hand.
- Client optimism is explicit now: a sent intent is overlaid on the folded
  state until the echoed entry arrives at its canonical seq and replaces it
  (or until any sequenced move touches the same card, which drops the stale
  overlay — fixing the old silent-divergence wart when the host refused an
  intent).
- The no-op guard moved into agni-core as `Table::changes(intent)` so the
  fold can reject no-op moves without cloning state; `apply` now delegates
  to it and a parity test pins the two together.
- The phase-B-adjacent prerequisite shipped early: spirit-node persists the
  node secret key at `<store>/identity/key` (hex, 0600) and rebinds it on
  every launch — stable node id, stable QR, and the signing key phase B and
  identity phase 2 both need.
- Verified by the determinism suite in `agni/net/tests/log_fold.rs`
  (identical folds across replicas, replay of an encoded log,
  shuffled-arrival order-from-seq-only, identical rejection of invalid
  entries, byte-stable re-encoding — it drives the fold through the
  `HostSession` sequencer, so it lives beside it in agni-net; the
  sequencer-free genesis guard stays in `agni/sim/src/log.rs`), the session
  tests in `agni/net/tests/session.rs`, and an end-to-end two-endpoint smoke
  over real iroh QUIC (`agni/net/tests/net_smoke.rs`, `#[ignore]`d so CI
  sandboxes skip it; run with
  `cargo test -p agni-net --test net_smoke -- --ignored`).

**Effort: 2–3 days. Verdict: done.**

**Phase B — signed entries, late-join and re-host from the log.** Persist
node keys (shared prerequisite with identity phase 2 — **done in phase A**),
sign entries, verify in the fold, bind seats to keys via `join`, hash-chain,
`Welcome` becomes log-prefix (**done in phase A**; snapshot optimization
optional), store finished logs as blobs,
reconnect-as-your-old-seat falls out (prove your key, resume your seat —
fixing another documented limitation). Owner-deals lands here (trusted, not
proven). **Effort: 4–6 days, less if it rides alongside identity phases 2–3.
Verdict: do it.**

**Phase C — sequencer election and failover.** Lowest-reachable-node-id
election on sequencer loss, longest-signed-chain resume, in-flight intents
re-proposed. Small once B exists, and it is the fix for the single worst
user-visible limitation in multiplayer.md — the frozen table and the fragile
phone host. **Effort: 2–3 days after B. Verdict: do it.**

**Phase D — commitments; remove the trusted dealer.** `deal` entries carry
per-card commitments (`blake3(canonical(face-record ‖ salt))`), `reveal`
opens them, decklists get committed when shared decks exist at all.
**Effort: 3–4 days plus the deck design that does not exist yet. Verdict:
defer.** Not wrong — the commitment slot is why `deal` carries ids from
phase A, so D never re-mints anything — but today there is no shared deck,
no competitive stakes, and a friends-at-a-table trust model where phases A–C
already *reduce* trust (the sequencer stops seeing everyone's hand). Build D
when a real game with real decks and strangers arrives, not before.

## The recommendation, stated plainly

- **Yes** to the deterministic append-only log: phases A–C are the redesign,
  A is cheap and pays immediately, and the ask's core intuition — the host
  already sequences, so make the log the authority and the host replaceable —
  is correct.
- **No** to CRDT as the convergence mechanism: card-game actions do not
  commute where it counts, the concurrency window is milliseconds among
  friends, and a replaceable sequencer gets total order without rollback or
  per-zone merge machinery. The only CRDT-ish structure retained is the
  grow-only reveal set, which is convergent by construction anyway.
- **The honest boss fight is hidden state**, and the pragmatic answer wins:
  one canonical face-free log for everyone, faces per-owner, reveals into
  the log, commitments reserved as a compatible phase-D upgrade rather than
  built now.
