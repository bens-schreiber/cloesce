import fs from "node:fs";
import path from "node:path";
import { Project, Type, SourceFile } from "ts-morph";

export type ExtractOptions = {
  cwd?: string;
  projectName?: string;
  version?: string;
  tsconfigPath?: string | undefined;
};

// ---- filesystem helpers ----------------------------------------------------

function readPkgMeta(cwd: string) {
  const pkgPath = path.join(cwd, "package.json");
  let projectName = path.basename(cwd);
  let version = "0.0.1";
  if (fs.existsSync(pkgPath)) {
    try {
      const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
      projectName = pkg.name ?? projectName;
      version = pkg.version ?? version;
    } catch {}
  }
  return { projectName, version };
}

const IGNORE_DIRS = new Set([
  "node_modules", ".git", "dist", "build", "out",
  ".next", ".turbo", "coverage", ".vercel", ".svelte-kit",
  ".output", ".cache"
]);

// Recursively collect only files strictly ending with `.cloesce.ts`, skipping vendor/build dirs 
function walkCloesceFiles(root: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) {
      if (!IGNORE_DIRS.has(entry.name)) out.push(...walkCloesceFiles(full));
    } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
      out.push(full);
    }
  }
  return out;
}

  // strip import stuff so comparisons are stable
function cleanTypeText(t: Type, sf: SourceFile) {
  return t.getText(sf).replace(/import\(".*?"\)\./g, "");
}

export enum TypeCode {
  Number = "number",
  String = "string",
  Boolean = "boolean",
  Date = "Date",
  D1Db = "D1Db",
  Response = "Response",
}

export namespace TypeCode {
  const basicTypeMap: Record<string, TypeCode> = {
    number: TypeCode.Number,
    string: TypeCode.String,
    boolean: TypeCode.Boolean,
    Date: TypeCode.Date,
    Response: TypeCode.Response,
    D1Db: TypeCode.D1Db,
  };

  export function fromType(t: Type, sf: SourceFile): TypeCode {
    const txt = cleanTypeText(t, sf);
    const symName = t.getSymbol()?.getName();

    // Robust primitive checks
    if (t.isNumber() || txt === "number") return TypeCode.Number;
    if (t.isString() || txt === "string") return TypeCode.String;
    if (t.isBoolean() || txt === "boolean") return TypeCode.Boolean;

    // Unwrap Promise<T> recursively
    if (symName === "Promise" && t.getTypeArguments().length === 1) {
      return fromType(t.getTypeArguments()[0], sf);
    }

    // Known names by text or symbol
    if (txt in basicTypeMap) return basicTypeMap[txt as keyof typeof basicTypeMap];
    if (symName === "Response" || txt === "Response") return TypeCode.Response;

    // D1Db types by suffix or symbol name
    if (txt.endsWith("D1Db") || symName === "D1Db") return TypeCode.D1Db;

    // Fallback
    return TypeCode.String;
  }
}

function isNullable(t: Type) {
  if (!t.isUnion()) return false;
  return t.getUnionTypes().some(u => u.isNull() || u.isUndefined());
}

function hasDecoratorNamed(node: { getDecorators(): any[] }, name: string): boolean {
  return node.getDecorators().some(d => {
    const n = d.getName() ?? d.getExpression().getText();
    // we should normalize things like "D1()", "ns.D1", etc.
    const plain = String(n).replace(/\(.*\)$/, "");
    return plain === name || plain.endsWith("." + name);
  });
}

// ---- main ------------------------------------------------------------------
export function extractModels(opts: ExtractOptions = {}) {
  const cwd = opts.cwd ?? process.cwd();
  const { projectName: pn, version: ver } = readPkgMeta(cwd);
  const projectName = opts.projectName ?? pn;
  const version = opts.version ?? ver;

  // Find *.cloesce.ts everywhere in project using cwd as the root
  const files = walkCloesceFiles(cwd);
  if (files.length === 0) {
    throw new Error(`No ".cloesce.ts" files found anywhere under "${cwd}"`);
  }

  const tsconfigPath =
    opts.tsconfigPath ??
    (fs.existsSync(path.join(cwd, "tsconfig.json")) ? path.join(cwd, "tsconfig.json") : undefined);

  const project = new Project({
    tsConfigFilePath: tsconfigPath,
    compilerOptions: tsconfigPath ? undefined : { target: 99, lib: ["es2022", "dom"] },
  });

  for (const f of files) project.addSourceFileAtPath(f);

  const models: any[] = [];

  for (const sf of project.getSourceFiles()) {
    for (const cls of sf.getClasses()) {
      // Only parse classes with @D1!
      if (!hasDecoratorNamed(cls, "D1")) continue;

      const className = cls.getName() ?? "<anonymous>";

      const attributes = cls.getProperties().map(prop => {
        const t = prop.getType();
        const entry: any = {
          name: prop.getName(),
          type: TypeCode.fromType(t, sf),
          nullable: isNullable(t),
        };
        if (hasDecoratorNamed(prop, "PrimaryKey")) entry.pk = true;
        return entry;
      });

      const methods = cls.getMethods().map(m => {
        const decos = m.getDecorators().map(d => d.getName() ?? d.getExpression().getText());
        const httpVerb =
          decos.includes("GET") ? "GET" :
          decos.includes("POST") ? "POST" :
          undefined;

        const parameters: any[] = [];
        for (const p of m.getParameters()) {
          const pt = p.getType();
          const raw = cleanTypeText(pt, sf);
          if (raw === "Request") continue; 
          if (raw.endsWith("D1Db") || p.getName() === "db") {
            parameters.push({ name: "d1", type: TypeCode.D1Db, nullable: isNullable(pt) || false });
          } else {
            parameters.push({
              name: p.getName(),
              type: TypeCode.fromType(pt, sf),
              nullable: isNullable(pt) || false,
            });
          }
        }

        return {
          name: m.getName(),
          static: m.isStatic(),
          http_verb: httpVerb,
          parameters,
          return: { type: TypeCode.fromType(m.getReturnType(), sf) },
        };
      });

      models.push({ [className]: { attributes, methods } });
    }
  }

  return {
    version,
    project_name: projectName,
    language: "typescript",
    models,
  };
}