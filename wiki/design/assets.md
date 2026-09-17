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

## Card backs and curated playmats

Card backs are runtime content, not files bundled with the application. The
browser requests the game's pinned content hash from the public content
service, without waiting for gateway discovery. Both Riftbound and Magic
use this path. Native clients use their local/peer cache and the upstream source.
Native artwork workers construct network deadlines inside the node runtime;
constructing a Tokio timer on their plain worker thread panics and strands the
art queue. Request card backs before faces so large decks do not delay them.

Curated playmat URLs identify immutable blobs. Browser downloads verify the
expected hash and decode the image before publishing it to the art cache.
Failures, invalid responses and timeouts remain retryable; a failed request
must not permanently mark the image as loaded. The download deadline is 20
seconds, with a three-second retry interval. Hidden cards and opponent hand
backs are refreshed when the art cache changes, even on an idle table.

Opening settings loads library previews without requiring a playmat selection.
The four former built-in playmats are no longer in the catalog; saved selections
of those names reset to felt. Legacy arbitrary URL entries are not fetched or
shared. Personal pictures use the separate flow below.

Artwork permissions are separate from the source-code license. See the
[artwork inventory](../artwork.md). Keep original files and embedded artist
credits intact when publishing content; only metadata belongs in this repository.

## Personal pictures

`os::picture` owns the desktop, browser and Android file-picker bridges.
`table::personal_playmat` validates input dimensions and allocation limits,
reencodes a bounded JPEG without source metadata, and stores only the owner's
copy under application configuration or browser local storage. It never calls
Spirit blob-store, journal, asset-index or `serve_bytes` APIs.

During a live table, Agni's personal-asset protocol holds one selected payload
in memory. Its revocable capability ticket travels through the existing
playmat roster field, never public discovery. Receivers verify its hash and
decode limits before putting it in their transient rendering cache. Requests
are bounded to eight seats, one pending request per seat, and a 30-second
retry interval. Changed selections, disconnects and the opponent opt-out cancel
pending requests and evict personal opponent images. The opt-out also gates
the ordinary curated/card-playmat fetch and rendering paths.

See [Agni's protocol contract](https://github.com/abysl/agni/blob/main/wiki/design/personal-assets.md)
and [curated service operation](https://github.com/abysl/agni/blob/main/wiki/published-content.md).
Application fetch guards do not by themselves secure a shared server; servers
must use a dedicated, reviewed published-only store.
