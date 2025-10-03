import { execSync, spawn } from "child_process";
import net from "net";
import fs from "fs/promises";

const PORT = 5002;
let controller: AbortController | undefined;

/**
 * Copies a fixture, runs migrations, builds, and starts a wrangler server.
 */
export async function startWrangler(fixturesPath: string) {
  await fs.cp(fixturesPath, ".generated", { recursive: true });

  runSync(
    "Applying D1 migrations",
    "echo y | npx wrangler d1 migrations apply db",
    { cwd: ".generated" },
  );
  runSync("Building Wrangler", "npx wrangler --config wrangler.toml build", {
    cwd: ".generated",
  });

  controller = new AbortController();
  spawn(
    "npx",
    ["wrangler", "dev", "--port", String(PORT), "--config", "wrangler.toml"],
    {
      cwd: ".generated",
      stdio: "pipe",
      signal: controller.signal,
    },
  ).once("error", () => {}); // ignore AbortError

  await new Promise((resolve) => setTimeout(resolve, 5000));
  console.log("Wrangler server ready ✅\n");
}

export async function stopWrangler() {
  controller?.abort();
  await fs.rm(".generated", { recursive: true, force: true });
}

function runSync(label: string, cmd: string, opts: { cwd?: string } = {}) {
  try {
    console.log(`${label}...`);
    execSync(cmd, { stdio: "inherit", ...opts });
    console.log("Ok ✅\n");
  } catch (err) {
    console.error(`${label} failed:`, err);
    process.exit(1);
  }
}
