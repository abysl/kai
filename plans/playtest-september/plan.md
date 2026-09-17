# Playtest follow-up

Audience: contributors reviewing the release and its regression coverage.

Fix the reported playtest failures without moving game decisions into Kai.
Keep existing selected decks across tables, recover missing card backs,
preview stack cards, browse public trash during choices, search large prompts,
and use consistent numbered yes/no choices. Add TCG Arena deck imports through
the shared importer. Complete Reflection copying, rune-payment selection,
Kennen/Reflow and Empower behavior in the rules plugin.

Rules and importer work is reviewed in Agni and synchronized with the separate
Riftbound plugin repository. Kai pins the compatible framework revision and
ships updated hardened modules. Run focused regression tests, native and
browser checks, formatting, then verify two-player browser interactions and
the release build. Never commit artwork, private stores, credentials or
deployment configuration.
