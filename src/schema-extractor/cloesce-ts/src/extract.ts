import fs from "node:fs";
import path from "node:path";
import { Project, Type, SourceFile } from "ts-morph";

export type ExtractOptions = {
  cwd?: string;
  projectName?: string;
  version?: string;
  dirNames?: string[];             
  extPattern?: RegExp;             
  tsconfigPath?: string | undefined;
};

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

function findModelsDir(cwd: string, dirNames: string[]) {
  for (const name of dirNames) {
    const p = path.join(cwd, name);
    if (fs.existsSync(p) && fs.statSync(p).isDirectory()) return p;
  }
  return null;
}

function walkFiles(root: string, extPattern: RegExp) {
  const out: string[] = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) out.push(...walkFiles(full, extPattern));
    else if (entry.isFile() && extPattern.test(entry.name)) out.push(full);
  }
  return out;
}

// ---- type helpers ----------------------------------------------------------
function cleanTypeText(t: Type, sf: SourceFile) {
  return t.getText(sf).replace(/import\(".*?"\)\./g, "");
}

/** 0=int, 1=string, 2=boolean, 3=Date ISO, 6=D1Db, 7=JSON/http */
function mapTypeToCode(t: Type, sf: SourceFile): number {
  const txt = cleanTypeText(t, sf);
  if (t.isNumber() || txt === "number") return 0;
  if (t.isString() || txt === "string") return 1;
  if (t.isBoolean() || txt === "boolean") return 2;
  if (txt === "Date") return 3;

  const sym = t.getSymbol();
  if (sym?.getName() === "Promise" && t.getTypeArguments().length === 1) {
    const inner = t.getTypeArguments()[0];
    const innerTxt = cleanTypeText(inner, sf);
    if (innerTxt === "Response") return 7;
    return mapTypeToCode(inner, sf);
  }

  if (txt === "Response") return 7;
  if (txt.endsWith("D1Db") || sym?.getName() === "D1Db") return 6;
  return 1;
}

function isNullable(t: Type) {
  if (!t.isUnion()) return false;
  return t.getUnionTypes().some(u => u.isNull() || u.isUndefined());
}

export function extractModels(opts: ExtractOptions = {}) {
  const cwd = opts.cwd ?? process.cwd();
  const { projectName: pn, version: ver } = readPkgMeta(cwd);
  const projectName = opts.projectName ?? pn;
  const version = opts.version ?? ver;

  // ONLY this directory name, users don't really need to define it
  const dirNames = opts.dirNames ?? ["models-cloesce"];
  const modelsDir = findModelsDir(cwd, dirNames);
  if (!modelsDir) {
    throw new Error(`Could not find models directory "models-cloesce" in ${cwd}`);
  }

  // ONLY this file suffix, same reason as above
  const extPattern = opts.extPattern ?? /\.cloesce\.ts$/;
  const files = walkFiles(modelsDir, extPattern);
  if (files.length === 0) {
    throw new Error(`No ".cloesce.ts" files found in "${modelsDir}"`);
  }

  const tsconfigPath =
    opts.tsconfigPath ??
    (fs.existsSync(path.join(cwd, "tsconfig.json")) ? path.join(cwd, "tsconfig.json") : undefined);

  const project = new Project({
    tsConfigFilePath: tsconfigPath,
    compilerOptions: tsconfigPath ? undefined : { target: 99 /* ESNext */, lib: ["es2022", "dom"] }
  });

  for (const f of files) project.addSourceFileAtPath(f);

  const models: any[] = [];

  for (const sf of project.getSourceFiles()) {
    for (const cls of sf.getClasses()) {
      const className = cls.getName() ?? "<anonymous>";

      const attributes = cls.getProperties().map(prop => {
        const t = prop.getType();
        const entry: any = {
          name: prop.getName(),
          type: mapTypeToCode(t, sf),
          nullable: isNullable(t)
        };
        if (prop.getDecorators().some(d => (d.getName() ?? d.getExpression().getText()) === "PrimaryKey")) {
          entry.pk = true;
        }
        return entry;
      });

      const methods = cls.getMethods().map(m => {
        const decos = m.getDecorators().map(d => d.getName() ?? d.getExpression().getText());
        const httpVerb = decos.includes("GET") ? "GET" : decos.includes("POST") ? "POST" : undefined;

        const parameters: any[] = [];
        for (const p of m.getParameters()) {
          const pt = p.getType();
          const raw = cleanTypeText(pt, sf);
          if (raw === "Request") continue;
          if (raw.endsWith("D1Db") || p.getName() === "db") {
            parameters.push({ name: "d1", type: 6 });
          } else {
            parameters.push({
              name: p.getName(),
              type: mapTypeToCode(pt, sf),
              nullable: isNullable(pt) || false
            });
          }
        }

        return {
          name: m.getName(),
          static: m.isStatic(),
          http_verb: httpVerb,
          parameters,
          return: { type: mapTypeToCode(m.getReturnType(), sf) }
        };
      });

      models.push({ [className]: { attributes, methods } });
    }
  }

  return {
    version,
    project_name: projectName,
    language: "typescript",
    models
  };
}
