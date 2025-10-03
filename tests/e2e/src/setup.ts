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

  await waitForPort(PORT, "localhost", 30_000, false);
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

/**
 * Waits for a port to be free or in use.
 * @param shouldBeFree true = wait for free, false = wait for in use
 */
function waitForPort(
  port: number,
  host: string,
  timeoutMs: number,
  shouldBeFree: boolean,
): Promise<void> {
  const start = Date.now();

  return new Promise((resolve, reject) => {
    const check = () => {
      const socket = net.createConnection({ port, host });
      socket.setTimeout(500);

      socket.once("connect", () => {
        socket.destroy();
        if (shouldBeFree) retry();
        else resolve();
      });

      socket.once("error", (err: NodeJS.ErrnoException) => {
        socket.destroy();
        if (!shouldBeFree && err.code !== "ECONNREFUSED") retry();
        else if (shouldBeFree && err.code === "ECONNREFUSED") resolve();
        else retry();
      });

      socket.once("timeout", () => {
        socket.destroy();
        retry();
      });
    };

    const retry = () => {
      if (Date.now() - start > timeoutMs)
        reject(new Error(`Timed out waiting for port ${port}`));
      else setTimeout(check, 200);
    };

    check();
  });
}
