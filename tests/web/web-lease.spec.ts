import { expect, test } from "@playwright/test";
import { join } from "node:path";
import { expectAgreement, expectSeat, expectStarted, expectSuccess, hostPlan, joinPlan, runDir, HOSTING_WAIT_MS, Seat } from "./lib";

test("web:web-lease — the joiner is a second page of the host's context", async ({ browser }) => {
  const dir = runDir("web-lease");
  const context = await browser.newContext();
  const hostsPlan = hostPlan("lease-host");
  const host = new Seat(await context.newPage(), "host", join(dir, "host"));
  await host.open(hostsPlan);
  const hosting = await host.waitFor("hosting", HOSTING_WAIT_MS);
  const joinersPlan = joinPlan(String(hosting.ticket), "lease-joiner");
  const joiner = new Seat(await context.newPage(), "joiner", join(dir, "joiner"));
  await joiner.open(joinersPlan);
  await joiner.waitFor("meshed", HOSTING_WAIT_MS);
  const hostNode = await host.nodeId();
  const joinerNode = await joiner.nodeId();
  expect(joinerNode, "the second tab must take the per-tab key, not the leased one").not.toBe(hostNode);
  const [hostOutcome, joinerOutcome] = await Promise.all([host.outcome(hostsPlan), joiner.outcome(joinersPlan)]);
  expectSuccess(host, hostOutcome, hostsPlan);
  expectSuccess(joiner, joinerOutcome, joinersPlan);
  expectAgreement(hostOutcome, joinerOutcome);
  expectSeat(host, 0);
  expectSeat(joiner, 1);
  expectStarted(host, hostsPlan);
  expectStarted(joiner, joinersPlan);
});
