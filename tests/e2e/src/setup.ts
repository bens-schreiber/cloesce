import { ChildProcess, execSync, spawn } from "child_process";
import fs from "fs/promises";
import kill from "tree-kill";

/**
 * Buffers all console output per Wrangler process and flushes it on demand.
 * Allows us to capture logs without interleaving them with other test output.
 */
class ConsoleBuffer {
  private logs: string[] = [];
  private readonly original = {
    log: console.log.bind(console),
    error: console.error.bind(console),
    warn: console.warn.bind(console),
  };

  constructor(private readonly prefix: string) {
    console.log = (...args) => this.capture("log", ...args);
    console.error = (...args) => this.capture("err", ...args);
    console.warn = (...args) => this.capture("warn", ...args);
  }

  capture(tag: string, ...args: any[]) {
    const msg = args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
    for (const line of msg.split("\n")) {
      if (line.trim()) this.logs.push(`[${this.prefix}|${tag}] ${line}`);
    }
  }

  flush() {
    const { log, error, warn } = this.original;
    console.log = log;
    console.error = error;
    console.warn = warn;
    if (this.logs.length > 0) {
      log(`\n--- [${this.prefix}] ---`);
      for (const line of this.logs) {
        log(line);
      }
      log(`--- [${this.prefix}] end ---\n`);
    }
  }
}

import { expect } from "vitest";
export function expectHttpResult(
  res: {
    ok: boolean;
  },
  message: string = "expect to be OK",
) {
  expect(res.ok, `${message}\n\n${JSON.stringify(res)}`).toBe(true);
}

export async function startWrangler(fixturesPath: string, workersUrl: string) {
  const prefix = fixturesPath.split("/").filter(Boolean).pop() ?? fixturesPath;
  const buffer = new ConsoleBuffer(prefix);

  let wranglerProcess: ChildProcess | null = null;
  try {
    wranglerProcess = await _startWrangler(fixturesPath, workersUrl, buffer);
  } catch (err) {
    if (wranglerProcess?.pid) {
      await killTree(wranglerProcess.pid);
    }
    buffer.capture("err", err instanceof Error ? err.message : String(err));
    buffer.flush();
    throw err;
  }

  return async () => {
    await killTree(wranglerProcess.pid!);
    buffer.flush();
  };
}

function killTree(pid: number): Promise<void> {
  return new Promise<void>((resolve) => {
    kill(pid, "SIGTERM", () => resolve());
  });
}

async function _startWrangler(
  fixturesPath: string,
  workersUrl: string,
  buffer: ConsoleBuffer,
): Promise<ChildProcess> {
  const d1Bindings = await getD1Bindings(fixturesPath);
  for (const binding of d1Bindings) {
    await runCmd(
      `Applying D1 migrations (${binding})`,
      `echo y | npx wrangler d1 migrations apply ${binding}`,
      { cwd: fixturesPath },
    );
  }

  await runCmd("Building Wrangler", "npx wrangler --config wrangler.toml build", {
    cwd: fixturesPath,
  });

  const port = portFromUrl(workersUrl);
  const wranglerProcess = spawn(
    "npx",
    [
      "wrangler",
      "dev",
      "--port",
      String(port),
      "--inspector-port",
      "0", // String(DEBUG_PORT),
      "--config",
      "wrangler.toml",
    ],
    { cwd: fixturesPath, stdio: "pipe" },
  );

  let exited: { code: number | null } | null = null;
  wranglerProcess.stdout?.on("data", (data) => buffer.capture("wrangler", data.toString()));
  wranglerProcess.stderr?.on("data", (data) => buffer.capture("wrangler", data.toString()));
  wranglerProcess.on("exit", (code) => {
    exited = { code };
    buffer.capture("wrangler", `⚠️ Wrangler process exited with code ${code}`);
  });

  await waitForServer(workersUrl, () => exited);
  console.log("Wrangler server ready ✅\n");

  return wranglerProcess;
}

async function waitForServer(
  workersUrl: string,
  exited: () => { code: number | null } | null,
  timeoutMs = 30_000,
): Promise<void> {
  const origin = new URL(workersUrl).origin;
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    const dead = exited();
    if (dead) {
      throw new Error(`Wrangler exited before becoming ready (code ${dead.code})`);
    }
    try {
      // Any HTTP response means the listener is up
      await fetch(origin, { signal: AbortSignal.timeout(1000) });
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }

  throw new Error(`Wrangler did not become ready within ${timeoutMs}ms at ${origin}`);
}

async function getD1Bindings(fixturesPath: string): Promise<string[]> {
  const cidlRaw = await fs.readFile(`${fixturesPath}/cidl.json`, "utf8");
  const cidl = JSON.parse(cidlRaw);
  return cidl.wrangler_env?.d1_bindings ?? [];
}

async function runCmd(label: string, cmd: string, opts: { cwd?: string } = {}) {
  console.log(`${label}...`);
  const out = execSync(cmd, { encoding: "utf8", stdio: "pipe", ...opts });
  if (out?.trim()) console.log(out.trim());
  console.log("Ok ✅\n");
}

function portFromUrl(url: string): number {
  const match = url.match(/:(\d+)/);
  if (!match) throw new Error(`Invalid workersUrl: ${url}`);
  return parseInt(match[1], 10);
}
