import { execFileSync } from "child_process";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const E2E_ROOT = path.resolve(__dirname, "..");
const FIXTURES_DIR = path.join(E2E_ROOT, "fixtures");
const CLOESCE_BIN = path.resolve(E2E_ROOT, "../../target/release/cloesce");

// Each fixture runs on its own worker in parallel, starting with this port seed.
const PORT_SEED = 5000;

export default function setup() {
  if (!fs.existsSync(CLOESCE_BIN)) {
    throw new Error(
      `cloesce binary not found at ${CLOESCE_BIN}. Run \`make build-src\` before the e2e tests.`,
    );
  }

  const fixtures = fs
    .readdirSync(FIXTURES_DIR, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => path.join(FIXTURES_DIR, e.name));

  fixtures.forEach((fixtureDir, i) => {
    const name = path.basename(fixtureDir);
    cleanFixture(fixtureDir);
    writeCloesceConfig(fixtureDir, PORT_SEED + i);

    // Compile and migrate every fixture such that the tests can use
    // all generated artifacts without worrying about setup.
    try {
      run(CLOESCE_BIN, ["compile"], fixtureDir);
      run(CLOESCE_BIN, ["migrate", "--all", "Initial"], fixtureDir);
    } catch (err) {
      const e = err as { stderr?: Buffer; stdout?: Buffer; message: string };
      const details = e.stderr?.toString() || e.stdout?.toString() || e.message;
      throw new Error(`Failed to compile fixture "${name}":\n${details}`);
    }

    normalizeMigrationNames(fixtureDir);
  });

  function run(bin: string, args: string[], cwd: string) {
    execFileSync(bin, args, { cwd, stdio: "pipe" });
  }
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
 * clean slate. Crucially, a leftover migrations directory would be read as the
 * "last migrated" state and yield an empty diff, so it must be cleared.
 */
function cleanFixture(fixtureDir: string) {
  for (const name of ["cidl.json", "wrangler.toml", "backend.ts", "client.ts"]) {
    fs.rmSync(path.join(fixtureDir, name), { force: true });
  }
  fs.rmSync(path.join(fixtureDir, "migrations"), { recursive: true, force: true });
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
