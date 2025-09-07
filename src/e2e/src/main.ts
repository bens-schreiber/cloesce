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

async function main() {
  // 1. Cloesce compiler
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

  // 2. Wrangler
  await runWrangler();

  // 3. Client tests
  await runClientTests();

  console.log("E2E tests successful ✅");
  process.exit(0);
}

async function runWrangler() {
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

async function runClientTests() {
  // Link module
  const outputJsDir = path.dirname(clientPath);
  const compiledClientPath = clientPath.replace(".ts", ".js");
  runSync(
    "Compiling generated client",
    `npx tsc ${clientPath} --outDir ${outputJsDir} --target ES2022 --module ESNext --moduleResolution node --allowSyntheticDefaultImports --esModuleInterop`
  );
  const mod = await import(pathToFileURL(compiledClientPath).href);

  const Person = mod.Person;
  assert(Person, "Failed to link client module");

  // Use generated post method
  const postResults = await Promise.all([
    Person.post("larry", "1-2-3"),
    Person.post("barry", null),
  ]);
  postResults.forEach((res, i) => assert(res.ok, JSON.stringify(res)));
  const [p1, p2] = postResults.map((res) =>
    Object.assign(new Person(), res.data)
  );

  // Use generated speak method
  const speakResults = await Promise.all([p1.speak(1), p2.speak(3)]);
  assert(speakResults[0].ok, JSON.stringify(speakResults[0]));
  assert(
    speakResults[0].data === "larry 1-2-3 1",
    `Expected "larry 1-2-3 1", got: ${JSON.stringify(speakResults[0].data)}`
  );
  assert(speakResults[1].ok, JSON.stringify(speakResults[1]));
  assert(
    speakResults[1].data === "barry null 3",
    `Expected "barry null 3", got: ${JSON.stringify(speakResults[1].data)}`
  );
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

function assert(condition: unknown, msg?: string): asserts condition {
  if (!condition) throw new Error(msg ?? "Assertion failed");
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

await main();
