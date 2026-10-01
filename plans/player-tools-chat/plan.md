# Shared player tools and table chat

Audience: contributors implementing and reviewing this feature.

Status: KAI-02 implementation completed on `task/kai-02-player-tools-chat`
from reconciled commit `1317cf0` plus reviewed harness commit `42fc703`.
Awaiting human review; no main merge, push, publication, or deployment.

## Contract

Human and automated seats use the same deck editor operations, importer,
catalog and search adapters. The model harness translates tools and schedules
requests; it does not own a second deck implementation or bypass host checks.
Table chat is seat-authenticated, bounded, session-local and delivered to human
and automated participants. Explain that messages seen by a model-backed seat
may be sent to its configured provider.

## Completed work

1. Add versioned table-chat messages and routing with bounded history and input.
2. Add shared deck discovery, authoring and import actions for native/browser.
3. Expose those actions in the human interface and thin model tools.
4. Route player chat and model replies through the same table connection.
5. Cover validation, private views, pre-game deck changes, async cancellation,
   search parsing and chat routing with synthetic fixtures.
6. Document capabilities and limits, run native/web checks and formatting,
   and prepare the coordinated release handoff.

The shipping version change and its `Cargo.lock` update remain a release-owner
step because KAI-02 was explicitly prohibited from updating locks. Delivery,
provider/device verification, and live publication remain downstream review or
KAI-03 work.

## Boundaries

No downloaded art or website payloads in source. Search uses supported public
pages with fixed site origins and bounded results; upstream access failures
are surfaced. No provider key is sent to a deck website or gateway. In-progress
games do not silently change decks; changes need the same new-game boundary
as human players. Private deployment configuration stays outside this repo.
