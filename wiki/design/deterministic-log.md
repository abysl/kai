# The game log boundary

Audience: developers considering changes to replay or session ordering.
Read [Kai architecture](../architecture.md) and Agni's
[architecture](https://github.com/abysl/agni/blob/main/wiki/design/architecture.md).

The deterministic log is implemented in Agni, not Kai. Kai renders the resulting
view and submits requests. A renderer-side state mutation bypasses validation,
ordering, replay, and the other players' copies.

The log records accepted inputs in a definite order. Starting from the same
initial state and module pins, replay must reach the same result. Network
arrival time and animation time are not a substitute for that ordering.

The host remains the sequencer. Deterministic replay helps replicas agree on
what an ordered log means; it does not prove that the host acted fairly or
prevent the host from withholding an action.

Hidden information adds a separate constraint: a log useful to one seat must
not automatically reveal every other seat's hand. Commitments, reveals, and
seat-specific faces must be reviewed together with replay.

For log changes, work in Agni's `sim/` and `net/`, add replay/visibility
tests, and review wire and snapshot versions. For Kai changes, test that the
visible result follows the accepted view without applying the action twice.
