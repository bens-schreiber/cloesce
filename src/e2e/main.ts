import { exec, execSync } from "child_process";
import path from "path";
import { pathToFileURL } from "url";
import net from "net";

const generatorPath = path.resolve("../generator");
const outputDir = path.join(process.cwd(), ".generated/");
const cidlPath = path.join(outputDir, "cidl.json");
const d1Path = path.join(process.cwd(), "migrations/d1.sql");
const workersPath = path.join(outputDir, "workers.ts");
const clientPath = path.join(outputDir, "client.ts");
const port = 5001;

// 1. Cloesce compiler
{
  runSync("Running the extractor", "npx cloesce");
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
    `cargo run generate client ${cidlPath} ${clientPath} localhost:${port}`,
    { cwd: generatorPath }
  );
}

// 2. Wrangler
{
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

// 3. Client
{
  const mod = await import(pathToFileURL(clientPath).href);
  const Person = mod.Person;
  assert(Person != undefined, "failed to link client module");

  let res1: Person = await Person.post("larry", "1-2-3");
  assert(res1.ok, JSON.stringify(res1));

  let res2: Person = await Person.post("barry", null);
  assert(res2.ok, JSON.stringify(res2));
}

console.log("e2e tests sucessfull ✅\n");
process.exit(0);

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

async function runAsync(label: string, cmd: string) {
  return new Promise<void>((resolve) => {
    console.log(`${label}...`);
    const child = exec(cmd);

    child.stdout?.pipe(process.stdout);
    child.stderr?.pipe(process.stderr);

    child.on("error", (err) => {
      console.error(`${label} failed:`, err);
      process.exit(1);
    });

    child.on("exit", (code) => {
      if (code !== 0) process.exit(code ?? 1);
      console.log("Ok ✅\n");
      resolve();
    });
  });
}

function assert(condition: unknown, msg?: string): asserts condition {
  if (!condition) {
    throw new Error(msg ?? "Assertion failed");
  }
}

function waitForPort(
  port: number,
  host: string,
  timeout_ms: number
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
        if (Date.now() - start > timeout_ms)
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
