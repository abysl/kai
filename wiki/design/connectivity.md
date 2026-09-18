# Connectivity test design

Audience: test-harness contributors. To run existing cases, start with the
[harness guide](../../tests/connectivity/README.md).

A case starts a host and a joiner on a selected pair of platforms, gives them
structured autoplay plans, and compares their outcomes. The purpose is to
exercise joining, seating, module exchange, and progression through a game.
Rules correctness remains the responsibility of rule tests.

The platform driver owns one client's process tree, logs, and cleanup.
The orchestrator owns the pair, deadlines, assertions, and combined report.
Use separate run directories and stores for every client.

A driver receives its run directory as the first argument and resolves it to
an absolute path before it changes directory, re-execs itself, or launches
the client: the directory belongs to the caller's location, not the
driver's. `KAI_DRIVER_RESOLVE_ONLY=1` makes a driver print that resolution
and exit, which is what `tests/connectivity/drivers/test-paths.sh` asserts
from a foreign working directory; the fast check runs that test.

Drivers translate the application's `KAI_EVENT` records into a common event
stream. Preserve raw platform logs for failures. An absent outcome is not a
successful game, and a preflight skip must be reported as a skip.

The browser and Android paths need their own artifacts and tools; a desktop
binary cannot stand in for them. External connectivity must be an explicit
test prerequisite rather than an undeclared private gateway assumption.

Add a new scenario by specifying its preconditions, input plans, expected
events, result comparison, and cleanup behavior. Verify both success and
timeout paths. This suite does not by itself cover three-player games,
malicious hosts, all browsers, or every reconnect scenario.
