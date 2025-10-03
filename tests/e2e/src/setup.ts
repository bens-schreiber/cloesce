import { exec, execSync, ChildProcess } from "child_process";
import net from "net";
import path from "path";
import fs from "fs/promises";

const PORT = 5002;
let wranglerProc: ChildProcess | null = null;

/**
 * Copies a fixture into the working directory, runs migrations, runs wrangler build, then starts
 * a wrangler server process tied to the parent processes lifetime.
 * @param fixturesPath The fixture to copy
 */
export async function startWrangler(fixturesPath: string) {
  await copyFolder(fixturesPath, ".generated");

  runSync(
    "Applying D1 migrations",
    "echo y | npx wrangler d1 migrations apply db",
    {
      cwd: ".generated",
    },
  );

  runSync("Building Wrangler", "npx wrangler --config wrangler.toml build", {
    cwd: ".generated",
  });

  wranglerProc = exec(
    `npx wrangler dev --port ${PORT} --config wrangler.toml`,
    {
      cwd: ".generated",
    },
  );

  process.once("exit", () => wranglerProc?.kill());
  process.once("SIGINT", () => {
    wranglerProc?.kill();
    process.exit(1);
  });

  wranglerProc?.stdout?.pipe(process.stdout);
  wranglerProc?.stderr?.pipe(process.stderr);

  await waitForPort(PORT, "localhost", PORT);
  console.log("Wrangler server ready ✅\n");
}

export async function stopWrangler() {
  if (wranglerProc) {
    console.log("Stopping Wrangler...");

    const closed = new Promise<void>((resolve) => {
      wranglerProc!.once("close", () => resolve());
    });

    wranglerProc.kill("SIGINT");
    await closed;

    wranglerProc = null;
  }

  try {
    await fs.rm(".generated", { recursive: true, force: true });
    console.log("Deleted .generated ✅");
  } catch (err) {
    console.warn("Could not delete .generated:", err);
  }
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

async function copyFolder(src: string, dest: string) {
  await fs.mkdir(dest, { recursive: true });

  for (const entry of await fs.readdir(src)) {
    const srcPath = path.join(src, entry);
    const destPath = path.join(dest, entry);
    const stat = await fs.stat(srcPath);

    if (stat.isFile()) {
      await fs.copyFile(srcPath, destPath);
    } else if (stat.isDirectory()) {
      // Recursively copy subdirectory
      await copyFolder(srcPath, destPath);
    }
  }
}

function waitForPort(
  port: number,
  host: string,
  timeoutMs: number,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const start = Date.now();

    const check = () => {
      const socket = net.connect({ port, host }, () => {
        socket.destroy();
        resolve();
      });

      socket.once("error", () => handleRetry(socket));
      socket.once("timeout", () => handleRetry(socket));
      socket.setTimeout(500);

      function handleRetry(socket: net.Socket) {
        socket.destroy();
        if (Date.now() - start > timeoutMs) {
          reject(new Error(`Timed out waiting for port ${port}`));
        } else {
          setTimeout(check, 200);
        }
      }
    };

    check();
  });
}
