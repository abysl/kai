import { expect, type Page } from "@playwright/test";
import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { timeoutS } from "./playwright.config";

export type Plan = {
  role: "host" | { join: { host: string } };
  game?: string;
  deck?: string;
  enforced?: boolean;
  brain?: { random: { seed: number } };
  until?: "winner" | "seated" | { turns: number };
  timeout_s?: number;
  turn_cap?: number;
  battlefield?: number;
  pace_ms?: number;
  name?: string;
  players?: number;
  exit?: boolean;
};

export type Event = { event: string; ms: number } & Record<string, unknown>;

export type Outcome = Event & {
  result: "winner" | "turns" | "seated" | "failed";
  winner?: number;
  turns: number;
  sent: number;
  refused: number;
  reason?: string;
  status: string[];
};

export type Mode = "host" | "join" | "both";

export const OUTCOME_GRACE_MS = 30_000;
export const HOSTING_WAIT_MS = 120_000;
export const BOOT_WAIT_MS = 120_000;
export const POLL_MS = 250;
export const SHOT_TIMEOUT_MS = 5_000;

const envNumber = (name: string, fallback: number) => {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  const value = Number(raw);
  if (!Number.isFinite(value)) throw new Error(`${name}=${raw} is not a number`);
  return value;
};

export const seed = () => envNumber("KAI_SEED", 7);

export const until = (): Plan["until"] => {
  const raw = process.env.KAI_UNTIL ?? "winner";
  if (raw === "winner" || raw === "seated") return raw;
  const turns = raw.match(/^turns:(\d+)$/);
  if (turns) return { turns: Number(turns[1]) };
  throw new Error(`KAI_UNTIL=${raw} is not winner, seated or turns:N`);
};

export const planFromEnv = (): Plan | null => {
  const raw = process.env.KAI_PLAN;
  if (raw === undefined || raw.trim() === "") return null;
  return JSON.parse(raw) as Plan;
};

export const roleOf = (plan: Plan): "host" | "join" => (plan.role === "host" ? "host" : "join");

export const mode = (): Mode => {
  const raw = (process.env.KAI_WEB_MODE ?? "").toLowerCase();
  if (raw === "host" || raw === "join" || raw === "both") return raw;
  if (raw !== "") throw new Error(`KAI_WEB_MODE=${raw} is not host, join or both`);
  const plan = planFromEnv();
  return plan ? roleOf(plan) : "both";
};

export const hostPlan = (name: string, seedValue = seed()): Plan => ({
  role: "host",
  brain: { random: { seed: seedValue } },
  until: until(),
  timeout_s: timeoutS,
  name,
});

export const joinPlan = (host: string, name: string, seedValue = seed() + 1): Plan => ({
  role: { join: { host } },
  brain: { random: { seed: seedValue } },
  until: until(),
  timeout_s: timeoutS,
  name,
});

export const runDir = (kind: string) => {
  const dir =
    process.env.KAI_RUN_DIR ??
    join(__dirname, "runs", `${kind}-${new Date().toISOString().replace(/[:.]/g, "-")}`);
  mkdirSync(dir, { recursive: true });
  return dir;
};

const kaiEvents = () => {
  const kai = (window as unknown as { kai?: { events(): string[] } }).kai;
  const loading = document.getElementById("loading")?.textContent ?? null;
  return { ready: kai !== undefined, lines: kai ? kai.events() : [], loading };
};

const kaiNodeId = () => {
  const kai = (window as unknown as { kai?: { nodeId(): string | null } }).kai;
  return kai ? kai.nodeId() : null;
};

export class Seat {
  readonly events: Event[] = [];
  readonly dir: string;
  openedAt = 0;
  private ready = false;
  private closed = false;
  private failure: string | null = null;

  constructor(
    readonly page: Page,
    readonly label: string,
    dir: string,
  ) {
    this.dir = dir;
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "events.jsonl"), "");
    writeFileSync(join(dir, "console.log"), "");
  }

  async open(plan: Plan) {
    writeFileSync(join(this.dir, "plan.json"), JSON.stringify(plan) + "\n");
    this.page.on("console", (message) => this.console(`${message.type()}: ${message.text()}`));
    this.page.on("pageerror", (error) => this.console(`pageerror: ${error.message}`));
    this.page.on("requestfinished", (request) => {
      if (!/\.(wasm|js)(\?|$)/.test(request.url())) return;
      const timing = request.timing();
      this.console(`fetched ${request.url()} in ${Math.round(timing.responseEnd)} ms`);
    });
    this.page.on("crash", () => {
      this.failure = "the page crashed";
    });
    this.page.on("close", () => {
      this.closed = true;
    });
    const url = "?autoplay=" + encodeURIComponent(JSON.stringify(plan));
    this.openedAt = Date.now();
    await this.page.goto(url, { waitUntil: "domcontentloaded" });
    await this.waitReady();
  }

  private console(line: string) {
    appendFileSync(join(this.dir, "console.log"), `${new Date().toISOString()} ${line}\n`);
    console.log(`[${this.label}] ${line}`);
  }

  private async waitReady() {
    const deadline = Date.now() + BOOT_WAIT_MS;
    while (Date.now() < deadline) {
      await this.poll();
      if (this.ready) {
        console.log(`[${this.label}] window.kai ready after ${Date.now() - this.openedAt} ms`);
        return;
      }
      await this.page.waitForTimeout(POLL_MS);
    }
    throw new Error(`${this.label}: window.kai never appeared within ${BOOT_WAIT_MS} ms`);
  }

  async poll() {
    if (this.closed) throw new Error(`${this.label}: the page closed`);
    if (this.failure) throw new Error(`${this.label}: ${this.failure}`);
    const seen = await this.page.evaluate(kaiEvents);
    if (seen.loading !== null && seen.loading.startsWith("failed to start")) {
      throw new Error(`${this.label}: the bundle did not start: ${seen.loading}`);
    }
    this.ready = seen.ready;
    for (const line of seen.lines) await this.record(line);
  }

  private async record(line: string) {
    let event: Event;
    try {
      event = JSON.parse(line) as Event;
    } catch (error) {
      throw new Error(`${this.label}: event line is not JSON (${error}): ${line}`);
    }
    this.events.push(event);
    appendFileSync(join(this.dir, "events.jsonl"), JSON.stringify({ at: Date.now(), ...event }) + "\n");
    console.log(`[${this.label}] KAI_EVENT ${line}`);
    if (event.event === "hosting") {
      writeFileSync(join(this.dir, "node_id"), `${event.node}\n`);
      writeFileSync(join(this.dir, "ticket"), `${event.ticket}\n`);
    }
    if (event.event === "seated" && !this.events.some((other) => other !== event && other.event === "seated")) {
      await this.shot("shot-seated.png");
    }
    if (event.event === "outcome") await this.shot("shot.png");
  }

  private async shot(name: string) {
    try {
      await this.page.screenshot({ path: join(this.dir, name), timeout: SHOT_TIMEOUT_MS });
    } catch (error) {
      this.console(`screenshot ${name} failed: ${error}`);
    }
  }

  find(name: string): Event | undefined {
    return this.events.find((event) => event.event === name);
  }

  last(name: string): Event | undefined {
    return [...this.events].reverse().find((event) => event.event === name);
  }

  async waitFor(name: string, timeoutMs: number): Promise<Event> {
    const deadline = Date.now() + timeoutMs;
    while (true) {
      await this.poll();
      const found = this.find(name);
      if (found) return found;
      const outcome = this.find("outcome");
      if (outcome && name !== "outcome") {
        throw new Error(`${this.label}: outcome ${JSON.stringify(outcome)} arrived before ${name}`);
      }
      if (Date.now() >= deadline) {
        throw new Error(`${this.label}: no ${name} within ${timeoutMs} ms; last events: ${this.tail()}`);
      }
      await this.page.waitForTimeout(POLL_MS);
    }
  }

  async outcome(plan: Plan): Promise<Outcome> {
    const budget = (plan.timeout_s ?? timeoutS) * 1000 + OUTCOME_GRACE_MS;
    const remaining = Math.max(POLL_MS, this.openedAt + budget - Date.now());
    return (await this.waitFor("outcome", remaining)) as Outcome;
  }

  async nodeId(): Promise<string> {
    const meshed = this.find("meshed");
    if (meshed) return String(meshed.node);
    const id = await this.page.evaluate(kaiNodeId);
    if (id === null) throw new Error(`${this.label}: no node id yet`);
    return id;
  }

  tail(count = 5): string {
    return this.events
      .slice(-count)
      .map((event) => JSON.stringify(event))
      .join(" | ");
  }
}

export const expectSuccess = (seat: Seat, outcome: Outcome, plan: Plan) => {
  expect(outcome.result, `${seat.label} outcome: ${JSON.stringify(outcome)}`).not.toBe("failed");
  const wanted = plan.until ?? "winner";
  const expected = typeof wanted === "string" ? wanted : "turns";
  const acceptable = expected === "turns" ? ["turns", "winner"] : [expected];
  expect(acceptable, `${seat.label} outcome ${outcome.result} is not the plan's until`).toContain(outcome.result);
};

export const expectAgreement = (host: Outcome, joiner: Outcome) => {
  expect(joiner.result, "the two sides disagree on the result").toBe(host.result);
  expect(joiner.winner, "the two sides disagree on the winner").toBe(host.winner);
  expect(joiner.turns, "the two sides disagree on the turn count").toBe(host.turns);
};

export const expectStarted = (seat: Seat, plan: Plan) => {
  if (plan.until === "seated") return;
  const started = seat.find("started");
  expect(started, `${seat.label} never reached turn 1`).toBeDefined();
  expect(started?.enforced, `${seat.label} started.enforced`).toBe(plan.enforced ?? true);
};

export const expectSeat = (seat: Seat, index: number) => {
  const seated = seat.find("seated");
  expect(seated, `${seat.label} never seated`).toBeDefined();
  expect(seated?.seat, `${seat.label} seat`).toBe(index);
};
