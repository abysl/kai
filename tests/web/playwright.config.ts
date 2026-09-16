import { defineConfig } from "@playwright/test";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

export const distDir = resolve(process.env.KAI_WEB_DIST ?? join(__dirname, "..", "..", "web", "dist"));
export const port = Number(process.env.KAI_WEB_PORT ?? 8123);
export const webUrl = process.env.KAI_WEB_URL ?? `http://127.0.0.1:${port}/`;
export const timeoutS = Number(process.env.KAI_TIMEOUT_S ?? 600);

const viewport = () => {
  const spec = process.env.KAI_WEB_VIEWPORT ?? "1280x800";
  const match = spec.match(/^(\d+)x(\d+)$/);
  if (!match) throw new Error(`KAI_WEB_VIEWPORT=${spec} is not WxH`);
  return { width: Number(match[1]), height: Number(match[2]) };
};

const bootAllowanceS = 180;

export const checkDist = () => {
  const hand = join(distDir, "hand.js");
  if (!existsSync(hand)) {
    throw new Error(`${hand} is missing: build the bundle first (devenv shell -- web-build)`);
  }
  if (!readFileSync(hand, "utf8").includes("kai_autoplay")) {
    throw new Error(`${hand} has no kai_autoplay export: rebuild the bundle (devenv shell -- web-build)`);
  }
  const engine = join(distDir, "engine.wasm");
  if (!existsSync(engine)) {
    throw new Error(`${engine} is missing: a browser host needs the pinned engine (devenv shell -- web-build)`);
  }
};

export default defineConfig({
  testDir: __dirname,
  testMatch: /.*\.spec\.ts/,
  timeout: (timeoutS + bootAllowanceS) * 1000,
  expect: { timeout: 30_000 },
  workers: 1,
  retries: 0,
  fullyParallel: false,
  reporter: [["list", { printSteps: true }]],
  outputDir: process.env.KAI_RUN_DIR ? join(process.env.KAI_RUN_DIR, "test-results") : join(__dirname, "test-results"),
  globalSetup: join(__dirname, "global-setup.ts"),
  webServer: process.env.KAI_WEB_URL
    ? undefined
    : {
        command: `node ${join(__dirname, "serve.mjs")} ${distDir} ${port}`,
        url: webUrl,
        reuseExistingServer: true,
        timeout: 30_000,
      },
  use: {
    baseURL: webUrl,
    headless: true,
    viewport: viewport(),
    launchOptions: {
      args: [
        "--use-gl=angle",
        "--use-angle=swiftshader",
        "--enable-unsafe-swiftshader",
        "--ignore-gpu-blocklist",
        "--disable-dev-shm-usage",
      ],
    },
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
});
