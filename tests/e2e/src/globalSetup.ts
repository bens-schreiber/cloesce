import { execFile } from "child_process";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const E2E_ROOT = path.resolve(__dirname, "..");
const FIXTURES_DIR = path.join(E2E_ROOT, "fixtures");
const CLOESCE_BIN = path.resolve(E2E_ROOT, "../../target/release/cloesce");

// Each fixture runs on its own worker in parallel, starting with this port seed.
const PORT_SEED = 5000;

export default async function setup() {
  if (!fs.existsSync(CLOESCE_BIN)) {
    throw new Error(
      `cloesce binary not found at ${CLOESCE_BIN}. Run \`make build-src\` before the e2e tests.`,
    );
  }

  console.info("Compiling fixtures...");
  const fixtures = fs
    .readdirSync(FIXTURES_DIR, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => path.join(FIXTURES_DIR, e.name));
  console.info(
    `Found ${fixtures.length} fixture(s): ${fixtures.map((f) => path.basename(f)).join(", ")}`,
  );

  const tasks = fixtures.map(async (fixtureDir, i) => {
    const name = path.basename(fixtureDir);
    cleanFixture(fixtureDir);
    writeCloesceConfig(fixtureDir, PORT_SEED + i);

    try {
      console.log(`[${name}] compiling...`);
      await run(CLOESCE_BIN, ["compile"], fixtureDir);

      console.log(`[${name}] migrating...`);
      await run(CLOESCE_BIN, ["migrate", "--all", "Initial"], fixtureDir);
    } catch (err) {
      const e = err as { stderr?: string; stdout?: string; message: string };
      const details = e.stderr || e.stdout || e.message;
      return { ok: false, message: `Failed to compile fixture "${name}":\n${details}` };
    }

    normalizeMigrationNames(fixtureDir);
    console.log(`[${name}] ready`);
    return { ok: true };
  });

  const results = await Promise.all(tasks);
  const failed = results.filter((r): r is { ok: false; message: string } => !r.ok);
  if (failed.length > 0) {
    const messages = failed.map((f) => f.message).join("\n");
    throw new Error(`Failed to compile ${failed.length} fixture(s):\n${messages}`);
  }
}

function run(bin: string, args: string[], cwd: string): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(bin, args, { cwd }, (error, stdout, stderr) => {
      if (error) {
        reject({ message: error.message, stdout, stderr });
      } else {
        resolve();
      }
    });
  });
}

// Basic `cloesce.jsonc` config with a dynamic port.
function writeCloesceConfig(fixtureDir: string, port: number) {
  const config = {
    src_paths: ["./"],
    workers_url: `http://localhost:${port}/api`,
    out_path: ".",
  };
  fs.writeFileSync(path.join(fixtureDir, "cloesce.jsonc"), JSON.stringify(config, null, 4) + "\n");
}

/**
 * Removes the generated artifacts from a previous run so each run starts from a
 * clean slate.
 */
function cleanFixture(fixtureDir: string) {
  for (const name of ["cidl.json", "wrangler.toml", "backend.ts", "client.ts"]) {
    fs.rmSync(path.join(fixtureDir, name), { force: true });
  }

  for (const dir of ["migrations", "dist", ".wrangler"]) {
    fs.rmSync(path.join(fixtureDir, dir), { recursive: true, force: true });
  }
}

/**
 * Migrations are emitted with a `<timestamp>_` prefix. The e2e fixtures generate
 * exactly one migration per binding, and committed worker entrypoints import them
 * by the stable name `Initial`, so strip the timestamp prefix here.
 */
function normalizeMigrationNames(fixtureDir: string) {
  const migrationsRoot = path.join(fixtureDir, "migrations");
  if (!fs.existsSync(migrationsRoot)) return;

  for (const binding of fs.readdirSync(migrationsRoot)) {
    const bindingDir = path.join(migrationsRoot, binding);
    if (!fs.statSync(bindingDir).isDirectory()) continue;

    for (const file of fs.readdirSync(bindingDir)) {
      const match = file.match(/^\d+_(.+)$/);
      if (!match) continue;
      fs.renameSync(path.join(bindingDir, file), path.join(bindingDir, match[1]));
    }
  }
}
