# Automated seats and waiting

Audience: contributors working on automated opponents across platforms.
Read [Kai architecture](../architecture.md) first.

An automated seat is a client of the same request path as a human player.
It must not edit game state directly or receive another seat's private view.

The driver owns a client session, interprets the current view, and sends
commands. A random policy, local automation, or a model-backed policy chooses
what to request. Keep session ownership separate from policy.

Automatic passing and forced answers use the shared offer logic. A wait
condition is a request to hold until the view changes in a relevant way, not
permission to loop indefinitely. Invalid waits must fall back or return a
clear reason instead of freezing play.

Use canned model responses for tests. They make command selection reproducible
without credentials, network access, or model usage charges. Cover exhausted
responses, invalid commands, refusals, disconnects, and shutdown while a
decision is pending.

Model-backed play may send card and game context to an external provider.
Credentials must come from runtime configuration and must never appear in
fixtures, logs, or source. The player's documentation must make this behavior
clear.

## Configuration and providers

`menu/ai_setup.rs` presents a responsive settings sheet for all lobby entry
points. Confirmation validates the key and model before hosting or starting a
seat. Cancellation has no session side effects. `ai/setup.rs` owns the draft
and asynchronous model-list result; changing providers detaches an old result
so it cannot overwrite the new provider's list or send its key elsewhere.

`ai/provider.rs` pins the two public API origins and parses tool-capability
metadata. Catalog requests are unauthenticated and never receive a key.
The chat client in `ai/nanogpt.rs` supports both providers; its existing module
name remains for command-line compatibility. Keys are runtime-only, debug
redacted, and never serialized. Provider failures use status-based messages,
not response bodies that might echo sensitive data.

Catalog contracts: [OpenRouter models](https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties)
and [NanoGPT models](https://docs.nano-gpt.com/api-reference/endpoint/models).
OpenRouter advertises tools through `supported_parameters`; NanoGPT uses
`capabilities.tool_calling` with `detailed=true`. Catalog availability is not
credential validation. Do not silently substitute a model or provider.

## Browser scheduling

`ai/local.rs` runs the native seat on its existing worker thread.
`ai/web_local.rs` supplies the browser implementation through the same local
seat interface. The browser driver owns a separate ClientSession replica and
exchanges Join, Intent and host frames through the same synthetic connection
path as the native seat. Never generate model context from HostState.

Browser HTTP uses fetch with an abort deadline. Async model decisions yield
during requests and between commands, allowing the host to validate and send
frames before the next tool result is assembled. The shared Brain builds
prompts and handles tools; the shared Pilot handles quiet passes and holds.
Browser chat and notes are memory-only; deck selection uses the lobby and the
browser deck library rather than filesystem imports.

Every decision carries a connection ID and an epoch. Stop/replacement, reset,
and rollback invalidate old decisions; pending undo consent suspends action
execution. A late HTTP response must never send commands or restore an old
brain. Provider errors pause the browser AI until the user intervenes.

Start in `src/ai/driver.rs`, `brain.rs`, `hold.rs`, and their neighboring tests.
Compile the browser target separately; a native check cannot detect accidental
filesystem-only APIs in a shared module. Browser lifecycle tests also compile
on native to verify message routing and cancellation without paid API calls.
