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
      `Failed to parse cloesce-config.json: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function getDecoratorPlainName(dec: import("ts-morph").Decorator): string {
  const n = dec.getName() ?? dec.getExpression().getText();
  return String(n).replace(/\(.*\)$/, "");
}

function getDecoratorArgText(dec: import("ts-morph").Decorator, idx: number): string | undefined {
  const args = dec.getArguments();
  if (!args[idx]) return undefined;
  const a = args[idx];

  // Identifier (e.g., ForeignKey(Dog))
  if ((a as any).getKind && (a as any).getKind() === 77 /* SyntaxKind.Identifier */) {
    return (a as any).getText();
  }

  // String literal (e.g., ForeignKey("Dog"))
  const txt = (a as any).getText?.();
  if (!txt) return undefined;
  // strip quotes if string
  const m = txt.match(/^['"](.*)['"]$/);
  return m ? m[1] : txt;
}

/** Turn a Type (Dog | import("...").Dog) into "Dog" if it's a class/interface name. */
function typeToModelName(t: Type, sf: SourceFile): string | undefined {
  const sym = t.getSymbol();
  const name = sym?.getName();
  if (name && !["Array", "Promise"].includes(name)) return name;

  const txt = cleanTypeText(t, sf);
  // Handle "Dog" or "Dog | undefined" etc
  const m = txt.match(/^[A-Za-z_]\w*/);
  return m ? m[0] : undefined;
}

/** Convenience: find a property by name on a class (works for class properties only). */
function findClassPropertyByName(cls: import("ts-morph").ClassDeclaration, name: string) {
  return cls.getProperties().find((p) => p.getName() === name);
}


const IGNORE_DIRS = new Set([
  "node_modules",
  ".git",
  "dist",
  "build",
  "out",
  ".next",
  ".turbo",
  "coverage",
  ".vercel",
  ".svelte-kit",
  ".output",
  ".cache",
]);

// Recursively collect only files strictly ending with `.cloesce.ts` from specified path
function walkCloesceFiles(root: string, searchPath: string): string[] {
  const fullPath = path.resolve(root, searchPath);

  if (!fs.existsSync(fullPath)) {
    console.warn(
      `Warning: Path "${searchPath}" specified in cloesce-config.json does not exist`,
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

export type CidlTypeJson = "Integer" | "Real" | "Text" | "Blob" | "D1Database";

const cidlTypeMap = {
  number: "Integer" as const,
  string: "Text" as const,
  boolean: "Integer" as const,
  D1Database: "D1Database" as const,
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
  sf: SourceFile,
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
  sf: SourceFile,
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
  name: string,
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
      `No "cloesce-config.json" found in "${cwd}". Please create a cloesce-config.json with a "source" field.`,
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
      `No ".cloesce.ts" files found in specified source path(s): ${paths}`,
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

      const fkMap = new Map<string, string>();

      const oneToOneMap = new Map<string, string>(); // relationPropName -> fkName

      for (const prop of cls.getProperties()) {
        for (const dec of prop.getDecorators()) {
          const dname = getDecoratorPlainName(dec);
          if (dname === "ForeignKey") {
            // Support ForeignKey(Dog) or ForeignKey("Dog", "id")
            const modelArg = getDecoratorArgText(dec, 0); // "Dog" or Dog
            if (modelArg) {
              fkMap.set(prop.getName(), modelArg);
            }
          } else if (dname === "OneToOne") {
            // @OneToOne("dogId")
            const fkName = getDecoratorArgText(dec, 0);
            if (fkName) {
              oneToOneMap.set(prop.getName(), fkName);
            }
          }
        }
      }
    
      const attributes: any[] = [];
      const consumedProps = new Set<string>();
    
      // Emit default attributes for properties that are not part of FK/relations
      function pushDefaultAttribute(prop: PropertyDeclaration) {
        const t = prop.getType();
        const { nullable } = getNullability(prop, sf);
        attributes.push({
          value: {
            name: prop.getName(),
            cidl_type: TypeCode.toCidlType(t, sf),
            nullable,
          },
          primary_key: hasDecoratorNamed(prop, "PrimaryKey"),
        });
        consumedProps.add(prop.getName());
      }

      for (const [relationPropName, fkName] of oneToOneMap.entries()) {
        const relationProp = findClassPropertyByName(cls, relationPropName);
        const fkProp = findClassPropertyByName(cls, fkName);
        if (!relationProp || !fkProp) {
          // If schema is malformed, fall back to default where possible
          if (relationProp && !consumedProps.has(relationPropName)) {
            pushDefaultAttribute(relationProp);
          }
          if (fkProp && !consumedProps.has(fkName)) {
            pushDefaultAttribute(fkProp);
          }
          continue;
        }
      
        // Determine target model
        let targetModel = fkMap.get(fkName);
        if (!targetModel) {
          const relT = relationProp.getType();
          targetModel = typeToModelName(relT, sf);
        }
        if (!targetModel) {
          // If still unknown, fall back to default props rather than crashing
          if (!consumedProps.has(relationPropName)) pushDefaultAttribute(relationProp);
          if (!consumedProps.has(fkName)) pushDefaultAttribute(fkProp);
          continue;
        }
      
        // Scalar FK entry (e.g., dogId: number)
        const fkType = fkProp.getType();
        const { nullable: fkNullable } = getNullability(fkProp, sf);
        attributes.push({
          foreign_key: { OneToOne: targetModel },
          value: {
            cidl_type: TypeCode.fromType(fkType, sf),
            name: fkName,             // NOTE: matches your desired JSON
            nullable: fkNullable,
          },
          primary_key: hasDecoratorNamed(fkProp, "PrimaryKey"),
        });
      
        // Relation entry (e.g., dog: Dog | undefined) — but "name" = fkName per your spec
        const relNullable = getNullability(relationProp, sf).nullable;
        attributes.push({
          foreign_key: { OneToOne: targetModel },
          value: {
            cidl_type: { model: targetModel },
            name: fkName,           
            nullable: relNullable,
          },
          primary_key: false,
        });
      
        consumedProps.add(fkName);
        consumedProps.add(relationPropName);
      }
    
      // 2b) Emit any remaining properties (that weren’t consumed by FK/OneToOne)
      for (const prop of cls.getProperties()) {
        if (consumedProps.has(prop.getName())) continue;
        pushDefaultAttribute(prop);
      }

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
        navigation_properties: [],
        data_sources: [],
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
