import fs from "node:fs";
import path from "node:path";
import {
  Project,
  Type,
  SourceFile,
  PropertyDeclaration,
  PropertySignature,
  MethodDeclaration,
  ParameterDeclaration,
} from "ts-morph";

export type ExtractOptions = {
  cwd?: string;
  projectName?: string;
  version?: string;
  tsconfigPath?: string | undefined;
};

type CloesceConfig = {
  source: string | string[];
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

function readCloesceConfig(cwd: string): CloesceConfig | null {
  const configPath = path.join(cwd, "cloesce-config.json");
  if (!fs.existsSync(configPath)) {
    return null;
  }

  try {
    const configContent = fs.readFileSync(configPath, "utf8");
    const config = JSON.parse(configContent) as CloesceConfig;

    // Validate config structure
    if (!config.source) {
      throw new Error('cloesce-config.json must contain a "source" field');
    }

    return config;
  } catch (error) {
    throw new Error(
      `Failed to parse cloesce-config.json: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}

// Recursively collect only files strictly ending with `.cloesce.ts` from specified path
function walkCloesceFiles(root: string, searchPath: string): string[] {
  const fullPath = path.resolve(root, searchPath);

  if (!fs.existsSync(fullPath)) {
    console.warn(
      `Warning: Path "${searchPath}" specified in cloesce-config.json does not exist`
    );
    return [];
  }

  const out: string[] = [];
  const stats = fs.statSync(fullPath);

  if (stats.isFile()) {
    // If it's a file, check if it ends with .cloesce.ts
    if (/\.cloesce\.ts$/i.test(fullPath)) {
      out.push(fullPath);
    }
  } else if (stats.isDirectory()) {
    // Recursively search directory
    function walkDir(dir: string) {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walkDir(full);
        } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
          out.push(full);
        }
      }
    }
    walkDir(fullPath);
  }

  return out;
}

function cleanTypeText(t: Type, sf: SourceFile) {
  return t.getText(sf).replace(/import\(".*?"\)\./g, "");
}

export type SqlTypeJson = "Integer" | "Real" | "Text" | "Blob";
export type CfTypeJson = "D1Database";
export type CidlTypeJson = { Sql: SqlTypeJson } | { Cf: CfTypeJson };

const cidlTypeMap = {
  number: { Sql: "Integer" } as const,
  string: { Sql: "Text" } as const,
  boolean: { Sql: "Integer" } as const,
  D1Database: { Cf: "D1Database" } as const,
} satisfies Record<string, CidlTypeJson>;

export namespace TypeCode {
  export function toCidlType(t: Type, sf: SourceFile): CidlTypeJson {
    const txt = cleanTypeText(t, sf);
    const symName = t.getSymbol()?.getName();

    // unwrap Promise<T>
    if (symName === "Promise" && t.getTypeArguments().length === 1) {
      return toCidlType(t.getTypeArguments()[0], sf);
    }

    // disregard nullish
    if (t.isUnion()) {
      const nonNullish = t
        .getUnionTypes()
        .find((u) => !u.isNull() && !u.isUndefined());

      if (nonNullish) return toCidlType(nonNullish, sf);

      throw new Error(`Union only contains null/undefined: ${txt}`);
    }

    if (txt in cidlTypeMap) {
      return cidlTypeMap[txt as keyof typeof cidlTypeMap];
    }

    throw new Error(`Unknown Cloesce type: ${txt}`);
  }
}

/**
 * True if a union type includes null or undefined.
 * This uses the Type API and works regardless of type text.
 */
function unionIncludesNullish(t: Type): {
  hasNull: boolean;
  hasUndefined: boolean;
} {
  if (!t.isUnion()) return { hasNull: false, hasUndefined: false };
  let hasNull = false,
    hasUndefined = false;
  for (const u of t.getUnionTypes()) {
    if (u.isNull()) hasNull = true;
    if (u.isUndefined()) hasUndefined = true;
  }
  return { hasNull, hasUndefined };
}

/**
 * Fallback textual check for declared type nodes (covers cases where the compiler
 * flattens or where strictNullChecks interfere).
 */
function typeNodeTextHasNullish(typeText?: string): {
  hasNull: boolean;
  hasUndefined: boolean;
} {
  if (!typeText) return { hasNull: false, hasUndefined: false };
  // token-boundary regex so we don't match substrings in identifiers
  const hasNull = /\bnull\b/.test(typeText);
  const hasUndefined = /\bundefined\b/.test(typeText);
  return { hasNull, hasUndefined };
}

/**
 * Extracts a robust "nullable" for a class property or interface property.
 * We treat:
 *   - optional (`?`) as DB-nullable (common mapping),
 *   - explicit unions with null/undefined as DB-nullable,
 *   - declared text that includes null/undefined as DB-nullable.
 * Also returns a reason tag for debugging/logging if you want it.
 */
function getNullability(
  prop: PropertyDeclaration | PropertySignature,
  sf: SourceFile
): {
  nullable: boolean;
  reason:
    | "optional"
    | "union-null"
    | "union-undefined"
    | "text-null"
    | "text-undefined"
    | null;
} {
  // 1) Syntactic optional (`foo?: ...`)
  if (
    (prop as PropertyDeclaration).hasQuestionToken &&
    (prop as PropertyDeclaration).hasQuestionToken()
  ) {
    return { nullable: true, reason: "optional" };
  }

  // 2) Type-level union check
  const t = (prop as PropertyDeclaration).getType
    ? (prop as PropertyDeclaration).getType()
    : (prop as PropertySignature).getType();
  const { hasNull, hasUndefined } = unionIncludesNullish(t);
  if (hasNull) return { nullable: true, reason: "union-null" };
  if (hasUndefined) return { nullable: true, reason: "union-undefined" };

  // 3) Textual fallback on declared type
  const node =
    (prop as PropertyDeclaration).getTypeNode?.() ??
    (prop as PropertySignature).getTypeNode?.();
  const typeText = node?.getText();
  const textCheck = typeNodeTextHasNullish(typeText);
  if (textCheck.hasNull) return { nullable: true, reason: "text-null" };
  if (textCheck.hasUndefined)
    return { nullable: true, reason: "text-undefined" };

  return { nullable: false, reason: null };
}

/**
 * Parameter nullability similar to properties (covers `arg?: T`, `T | null`, `T | undefined`).
 */
function getParamNullability(
  param: ParameterDeclaration,
  sf: SourceFile
): boolean {
  // `?` ⇒ treat as DB-nullable
  if (param.hasQuestionToken()) return true;

  const t = param.getType();

  // union T | null/undefined
  const { hasNull, hasUndefined } = unionIncludesNullish(t);
  if (hasNull || hasUndefined) return true;

  // textual fallback on declared type (covers odd inference cases)
  const typeText = param.getTypeNode()?.getText();
  const textCheck = typeNodeTextHasNullish(typeText);
  return textCheck.hasNull || textCheck.hasUndefined;
}

function hasDecoratorNamed(
  node: { getDecorators(): any[] },
  name: string
): boolean {
  return node.getDecorators().some((d) => {
    const n = d.getName() ?? d.getExpression().getText();
    // we should normalize things like "D1()"
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

  // Read cloesce-config.json
  const config = readCloesceConfig(cwd);
  if (!config) {
    throw new Error(
      `No "cloesce-config.json" found in "${cwd}". Please create a cloesce-config.json with a "source" field.`
    );
  }

  // Normalize source to array
  const sourcePaths = Array.isArray(config.source)
    ? config.source
    : [config.source];

  // Find *.cloesce.ts files in specified paths
  const files: string[] = [];
  for (const sourcePath of sourcePaths) {
    files.push(...walkCloesceFiles(cwd, sourcePath));
  }

  if (files.length === 0) {
    const paths = sourcePaths.join(", ");
    throw new Error(
      `No ".cloesce.ts" files found in specified source path(s): ${paths}`
    );
  }

  const tsconfigPath =
    opts.tsconfigPath ??
    (fs.existsSync(path.join(cwd, "tsconfig.json"))
      ? path.join(cwd, "tsconfig.json")
      : undefined);

  const project = new Project({
    tsConfigFilePath: tsconfigPath,
    compilerOptions: tsconfigPath
      ? undefined
      : { target: 99, lib: ["es2022", "dom"] },
  });

  for (const f of files) project.addSourceFileAtPath(f);

  const models: any[] = [];

  for (const sf of project.getSourceFiles()) {
    for (const cls of sf.getClasses()) {
      // Only parse classes with @D1!
      if (!hasDecoratorNamed(cls, "D1")) continue;

      const className = cls.getName() ?? "<anonymous>";

      const attributes = cls.getProperties().map((prop) => {
        const t = prop.getType();
        const { nullable } = getNullability(prop, sf);

        const entry: any = {
          value: {
            name: prop.getName(),
            cidl_type: TypeCode.toCidlType(t, sf),
            nullable,
          },
        };

        if (hasDecoratorNamed(prop, "PrimaryKey")) {
          entry.primary_key = true;
        } else {
          entry.primary_key = false;
        }
        return entry;
      });

      const methods = cls.getMethods().map((m) => {
        const decos = m
          .getDecorators()
          .map((d) => d.getName() ?? d.getExpression().getText());
        const HTTP_VERBS = ["GET", "POST", "PUT", "PATCH", "DELETE"];
        const httpVerb =
          HTTP_VERBS.find((verb) => decos.includes(verb)) || undefined;

        const parameters: any[] = [];
        for (const p of m.getParameters()) {
          const pt = p.getType();

          const raw = cleanTypeText(pt, sf);
          if (raw === "Request") continue;

          const nullable = getParamNullability(p, sf);
          parameters.push({
            name: p.getName(),
            cidl_type: TypeCode.toCidlType(pt, sf),
            nullable,
          });
        }

        return {
          name: m.getName(),
          is_static: m.isStatic(),
          http_verb: httpVerb,
          parameters,
        };
      });

      const sourcePath = path.relative(cwd, sf.getFilePath());

      models.push({
        name: className,
        source_path: sourcePath,
        attributes,
        methods,
      });
    }
  }

  return {
    version,
    project_name: projectName,
    language: "TypeScript",
    models,
  };
}
