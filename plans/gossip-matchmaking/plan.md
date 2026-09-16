# Gossip matchmaking

Audience: contributors implementing and reviewing the two-player queue.

Players opt in from a game's lobby after choosing a deck and table settings.
The existing Spirit gossip mesh distributes waiting-table advertisements.
An Agni protocol reserves one opponent using authenticated endpoint identities.
No deck lists, cards, keys, or player names belong in matchmaking advertisements.

Matching compares canonical table configuration, game identity, session wire
version, and pinned engine/plugin hashes. Default and explicit two-player
defaults are equivalent. A fresh search nonce prevents stale claims from
reserving a later search. The lower endpoint identity hosts; outgoing and
incoming reservations are mutually exclusive. Reservations expire, and
cancelled attempts cannot deliver a late successful join to a new session.

The lobby keeps settings fixed during a search and offers cancellation.
Successful peers enter the existing session/dealing flow. Admission remains
limited to the matched opponent, including reconnects. Ordinary friend tables
continue to work without matchmaking.

Verification covers state-machine contention, expiry, cancellation, incompatible
settings, authenticated network exchange, native/browser compilation, lobby
tests, formatting, and desktop/phone layout. Release as a minor version after
review; confirm the public web build and Android artifact after deployment.
