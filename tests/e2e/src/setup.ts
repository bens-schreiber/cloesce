import { ChildProcess, execSync, spawn } from "child_process";
import fs from "fs/promises";
import kill from "tree-kill";

// const DEBUG_PORT = 9230;

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
    const msg = args
      .map((a) => (typeof a === "string" ? a : JSON.stringify(a)))
      .join(" ");
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

export function withRes(message: string, res: any): string {
  return `${message}\n\n${JSON.stringify(res)}`;
}

export async function startWrangler(fixturesPath: string, workersUrl: string) {
  const prefix = fixturesPath.split("/").filter(Boolean).pop() ?? fixturesPath;
  const buffer = new ConsoleBuffer(prefix);

  let wranglerProcess: ChildProcess | null = null;
  try {
    wranglerProcess = await _startWrangler(fixturesPath, workersUrl, buffer);
  } catch (err) {
    if (wranglerProcess) kill(wranglerProcess.pid!, "SIGTERM");
    buffer.capture("err", err instanceof Error ? err.message : String(err));
    buffer.flush();
    throw err;
  }

  return async () => {
    await new Promise<void>((resolve, reject) => {
      kill(wranglerProcess!.pid!, "SIGTERM", (err) =>
        err ? reject(err) : resolve(),
      );
    });
    buffer.flush();
  };
}

async function _startWrangler(
  fixturesPath: string,
  workersUrl: string,
  buffer: ConsoleBuffer,
): Promise<ChildProcess> {
  await fs.rm(`${fixturesPath}/.wrangler`, { recursive: true, force: true });
  await fs.rm(`${fixturesPath}/dist`, { recursive: true, force: true });

  const d1Bindings = await getD1Bindings(fixturesPath);
  for (const binding of d1Bindings) {
    await runCmd(
      `Applying D1 migrations (${binding})`,
      `echo y | npx wrangler d1 migrations apply ${binding}`,
      { cwd: fixturesPath },
    );
  }

  await runCmd(
    "Building Wrangler",
    "npx wrangler --config wrangler.toml build",
    {
      cwd: fixturesPath,
    },
  );

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

  wranglerProcess.stdout?.on("data", (data) =>
    buffer.capture("wrangler", data.toString()),
  );
  wranglerProcess.stderr?.on("data", (data) =>
    buffer.capture("wrangler", data.toString()),
  );
  wranglerProcess.on("exit", (code) =>
    buffer.capture("wrangler", `⚠️ Wrangler process exited with code ${code}`),
  );

  await new Promise((resolve) => setTimeout(resolve, 5000));
  console.log("Wrangler server ready ✅\n");

  return wranglerProcess;
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
