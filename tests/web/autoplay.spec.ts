import { expect, test, type Browser } from "@playwright/test";
import { join } from "node:path";
import {
  expectAgreement,
  expectSeat,
  expectStarted,
  expectSuccess,
  hostPlan,
  joinPlan,
  mode,
  planFromEnv,
  roleOf,
  runDir,
  HOSTING_WAIT_MS,
  Seat,
  type Plan,
} from "./lib";

const selected = mode();

const seatIn = async (browser: Browser, label: string, dir: string) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  return new Seat(page, label, dir);
};

const planFor = (role: "host" | "join"): Plan => {
  const given = planFromEnv();
  if (given) {
    if (roleOf(given) !== role) throw new Error(`KAI_PLAN has role ${JSON.stringify(given.role)}, the mode wants ${role}`);
    return given;
  }
  if (role === "host") return hostPlan("web-host");
  const host = process.env.KAI_JOIN_HOST;
  if (!host) throw new Error("join mode needs KAI_PLAN with a join role or KAI_JOIN_HOST");
  return joinPlan(host, "web-joiner");
};

test.describe("autoplay", () => {
  test.skip(selected !== "host", "KAI_WEB_MODE is not host");
  test("host", async ({ browser }) => {
    const plan = planFor("host");
    const seat = await seatIn(browser, "host", runDir("host"));
    await seat.open(plan);
    const hosting = await seat.waitFor("hosting", HOSTING_WAIT_MS);
    console.log(`host node ${hosting.node} ticket in ${join(seat.dir, "ticket")}`);
    const outcome = await seat.outcome(plan);
    expectSuccess(seat, outcome, plan);
    if (plan.until !== "seated") expectSeat(seat, 0);
  });
});

test.describe("autoplay", () => {
  test.skip(selected !== "join", "KAI_WEB_MODE is not join");
  test("join", async ({ browser }) => {
    const plan = planFor("join");
    const seat = await seatIn(browser, "joiner", runDir("join"));
    await seat.open(plan);
    const outcome = await seat.outcome(plan);
    expectSuccess(seat, outcome, plan);
    expectSeat(seat, 1);
  });
});

test.describe("autoplay", () => {
  test.skip(selected !== "both", "KAI_WEB_MODE is not both");
  test("both", async ({ browser }) => {
    const dir = runDir("both");
    const hostsPlan = hostPlan("web-host");
    const host = await seatIn(browser, "host", join(dir, "host"));
    await host.open(hostsPlan);
    const hosting = await host.waitFor("hosting", HOSTING_WAIT_MS);
    const joinersPlan = joinPlan(String(hosting.ticket), "web-joiner");
    const joiner = await seatIn(browser, "joiner", join(dir, "joiner"));
    await joiner.open(joinersPlan);
    await joiner.waitFor("meshed", HOSTING_WAIT_MS);
    const hostNode = await host.nodeId();
    const joinerNode = await joiner.nodeId();
    expect(joinerNode, "two contexts of one origin must be two peers").not.toBe(hostNode);
    expect(hostNode).toBe(String(hosting.node));
    const [hostOutcome, joinerOutcome] = await Promise.all([host.outcome(hostsPlan), joiner.outcome(joinersPlan)]);
    expectSuccess(host, hostOutcome, hostsPlan);
    expectSuccess(joiner, joinerOutcome, joinersPlan);
    expectAgreement(hostOutcome, joinerOutcome);
    expectSeat(host, 0);
    expectSeat(joiner, 1);
    expectStarted(host, hostsPlan);
    expectStarted(joiner, joinersPlan);
  });
});
