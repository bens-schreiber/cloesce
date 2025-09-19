import { exec, execSync } from "child_process";
import path from "path";
import { pathToFileURL } from "url";
import net from "net";

const generatorPath = path.resolve("../generator");
const outputDir = path.join(process.cwd(), ".generated");
const cidlPath = path.join(outputDir, "cidl.json");
const d1Path = path.join(process.cwd(), "migrations/d1.sql");
const workersPath = path.join(outputDir, "workers.ts");
const clientPath = path.join(outputDir, "client.ts");
const port = 5001;

export function compile() {
  // runSync("Running the extractor", "npx cloesce");
  runSync("Generating d1", `cargo run generate d1 ${cidlPath} ${d1Path}`, {
    cwd: generatorPath,
  });
  runSync(
    "Generating workers",
    `cargo run generate workers ${cidlPath} ${workersPath}`,
    { cwd: generatorPath }
  );
  runSync(
    "Generating client",
    `cargo run generate client ${cidlPath} ${clientPath} http://localhost:${port}/api`,
    { cwd: generatorPath }
  );
}

export async function startWrangler() {
  runSync(
    "Running wrangler migrate",
    "echo y | npx wrangler d1 migrations apply e2e_db"
  );

  runSync("Running wrangler build", "npx wrangler build");

  const wrangler = exec(`npx wrangler dev --port ${port}`);
  process.on("exit", () => wrangler.kill());
  process.on("SIGINT", () => {
    wrangler.kill();
    process.exit(1);
  });

  wrangler.stdout?.pipe(process.stdout);
  wrangler.stderr?.pipe(process.stderr);

  await waitForPort(port, "localhost", 5000);
  console.log("Wrangler server ready ✅\n");
}

export async function linkGeneratedModule() {
  const outputJsDir = path.dirname(clientPath);
  const compiledClientPath = clientPath.replace(".ts", ".js");
  runSync(
    "Compiling generated client",
    `npx tsc ${clientPath} --outDir ${outputJsDir} --target ES2022 --module ESNext --moduleResolution node --allowSyntheticDefaultImports --esModuleInterop`
  );
  return await import(pathToFileURL(compiledClientPath).href);
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

function waitForPort(
  port: number,
  host: string,
  timeoutMs: number
): Promise<void> {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const check = () => {
      const socket = net.connect({ port, host }, () => {
        socket.destroy();
        resolve();
      });

      const cleanupAndRetry = () => {
        socket.destroy();
        if (Date.now() - start > timeoutMs)
          reject(new Error(`Timed out waiting for port ${port}`));
        else setTimeout(check, 200);
      };

      socket.once("error", cleanupAndRetry);
      socket.once("timeout", cleanupAndRetry);
      socket.setTimeout(500);
    };
    check();
  });
}
