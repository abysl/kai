# Loading card images

Audience: contributors debugging missing or repeated asset downloads.
Read [Kai architecture](../architecture.md) first.

A deck can identify a card before its image is available. Image loading must
not block the game decision or make the simulation depend on a URL response.

Spirit stores bytes by hash. An asset index maps an application asset key to a
blob that a peer can provide. Kai can consult local and peer-held data before
using an external source supplied by the importer.

These are different failures: no asset mapping, a known blob not held locally,
an unreachable provider, and bytes that fail verification. Keep diagnostics
specific enough to distinguish them.

A browser may need a configured gateway when the upstream source disallows
cross-origin requests. That is an application integration choice, not an
assumption that contributors have access to a particular hosted service.

Test missing art, late arrival, repeated requests, invalid bytes, and a peer
disconnect during loading. The table should remain usable with placeholders.
Do not commit downloaded card images or local asset journals.
