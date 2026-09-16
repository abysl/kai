# Automated seats and waiting

Audience: contributors working on native automated opponents.
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

Start in `src/ai/driver.rs`, `brain.rs`, `hold.rs`, and their neighboring
tests. Browser builds do not provide the native automated-seat implementation.
