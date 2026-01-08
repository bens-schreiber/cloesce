import { ChildProcess, execSync, spawn } from "child_process";
import fs from "fs/promises";
import kill from "tree-kill";

const PORT = 5002;
const DEBUG_PORT = 9230;
let wranglerProcess: ChildProcess;

/**
 * Copies a fixture, runs migrations, builds, and starts a wrangler server.
 */
export async function startWrangler(
  fixturesPath: string,
  withMigrations: boolean = true,
) {
  await fs.rm(".generated", { recursive: true, force: true });
  await fs.cp(fixturesPath, ".generated", { recursive: true });

  if (withMigrations) {
    await runCmd(
      "Applying D1 migrations",
      "echo y | npx wrangler d1 migrations apply db",
      { cwd: ".generated" },
    );
  }

  await runCmd(
    "Building Wrangler",
    "npx wrangler --config wrangler.toml build",
    {
      cwd: ".generated",
    },
  );

  wranglerProcess = spawn(
    "npx",
    [
      "wrangler",
      "dev",
      "--port",
      String(PORT),
      "--config",
      "wrangler.toml",
      "--inspector-port",
      String(DEBUG_PORT),
    ],
    {
      cwd: ".generated",
      stdio: "inherit",
    },
  );

  wranglerProcess.stdout?.on("data", (data) => {
    console.log(`[wrangler stdout]: ${data}`);
  });

  wranglerProcess.stderr?.on("data", (data) => {
    console.error(`[wrangler stderr]: ${data}`);
  });

  wranglerProcess.on("exit", (code) => {
    console.log(`⚠️ Wrangler process exited with code ${code}`);
  });

  await new Promise((resolve) => setTimeout(resolve, 5000));
  console.log("Wrangler server ready ✅\n");
}

/**
 * Kills the running wrangler process via `kill-tree`
 */
export async function stopWrangler() {
  await new Promise<void>((resolve, reject) => {
    kill(wranglerProcess.pid!, "SIGTERM", (err) => {
      if (err) reject(err);
      else resolve();
    });
  });
}

export function withRes(message: string, res: any): string {
  return `${message}\n\n${JSON.stringify(res)}`;
}

async function runCmd(label: string, cmd: string, opts: { cwd?: string } = {}) {
  try {
    console.log(`${label}...`);
    execSync(cmd, { stdio: "inherit", ...opts });
    console.log("Ok ✅\n");
  } catch (err) {
    console.error(`${label} failed:`, err);
    await stopWrangler();
  }
}
