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
  SyntaxKind,
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

function isD1DbType(t: Type, sf: SourceFile): boolean {
  // robust even if the type shows up as import("...").D1Db
  const txt = cleanTypeText(t, sf);
  const sym = t.getSymbol()?.getName();
  return (
    txt === "D1Db" ||
    sym === "D1Db" ||
    TypeCode.fromType(t, sf) === TypeCode.D1Db
  );
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

// Normalize identifiers so "treatId" matches "treat_id" (and vice-versa).
function normalizeIdent(s: string): string {
  return s.replace(/[_\s]/g, "").toLowerCase();
}

/** Find a class property by its exact name, or a normalized alias (snake↔camel). */
function findClassPropertyByAlias(
  cls: import("ts-morph").ClassDeclaration,
  wanted: string,
) {
  // First try exact match
  const direct = cls.getProperties().find(p => p.getName() === wanted);
  if (direct) return direct;

  // Fallback: normalized match
  const wantedNorm = normalizeIdent(wanted);
  return cls.getProperties().find(p => normalizeIdent(p.getName()) === wantedNorm);
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
          if (!IGNORE_DIRS.has(entry.name)) {
            walkDir(full);
          }
        } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
          out.push(full);
        }
      }
    }
    walkDir(fullPath);
  }

  return out;
}

// strip import stuff so comparisons are stable
function cleanTypeText(t: Type, sf: SourceFile) {
  return t.getText(sf).replace(/import\(".*?"\)\./g, "");
}

export enum TypeCode {
  Number = "Integer",
  String = "Text",
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
    if (t.isNumber()) return TypeCode.Number;
    if (t.isString()) return TypeCode.String;
    if (t.isBoolean()) return TypeCode.Boolean;

    // Unwrap Promise<T> recursively
    if (symName === "Promise" && t.getTypeArguments().length === 1) {
      return fromType(t.getTypeArguments()[0], sf);
    }

    // Known names by text or symbol
    if (txt in basicTypeMap)
      return basicTypeMap[txt as keyof typeof basicTypeMap];

    // Fallback
    return TypeCode.String;
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

// ---- helpers for data sources ---------------------------------------------
type TreeNode = {
  value: {
    name: string;
    cidl_type: { Model: string };
    nullable: boolean;
  };
  data_sources: TreeNode[] | {};
};

function buildIncludeTreeFromObjectLiteral(
  obj: import("ts-morph").ObjectLiteralExpression,
  currentClass: import("ts-morph").ClassDeclaration,
  sf: SourceFile,
): TreeNode[] {
  const result: TreeNode[] = [];

  for (const propAssign of obj.getProperties()) {
    if (!propAssign.isKind(SyntaxKind.PropertyAssignment)) continue;

    const relationKey = propAssign.getName(); // e.g., "dog" or "treat"
    let relationProp = findClassPropertyByAlias(currentClass, relationKey);
    
    // If not found, try with 's' appended (singular -> plural)
    if (!relationProp && !relationKey.endsWith('s')) {
      relationProp = findClassPropertyByAlias(currentClass, relationKey + 's');
    }
    // Or try removing 's' (plural -> singular)
    if (!relationProp && relationKey.endsWith('s')) {
      relationProp = findClassPropertyByAlias(currentClass, relationKey.slice(0, -1));
    }
    
    if (!relationProp) {
      console.log(`  Warning: Could not find property "${relationKey}" in class ${currentClass.getName()}`);
      continue;
    }

    // Determine target model
    let targetModel: string | undefined;
    
    // Check for OneToMany decorator first
    const oneToManyDec = relationProp.getDecorators().find(d => getDecoratorPlainName(d) === "OneToMany");
    if (oneToManyDec) {
      // For OneToMany, we need to extract the model from the array type
      const propType = relationProp.getType();
      const typeText = cleanTypeText(propType, sf);
      // Extract model name from array type (e.g., "Dog[]" -> "Dog")
      const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
      if (arrayMatch) {
        targetModel = arrayMatch[1];
      }
    }
    
    // If not OneToMany, check ForeignKey decorator
    if (!targetModel) {
      const fkDec = relationProp.getDecorators().find(d => getDecoratorPlainName(d) === "ForeignKey");
      targetModel = fkDec ? getDecoratorArgText(fkDec, 0) : undefined;
    }
    
    // If still no model, try to infer from type
    if (!targetModel) {
      targetModel = typeToModelName(relationProp.getType(), sf);
    }
    
    if (!targetModel) continue;

    const { nullable } = getNullability(relationProp, sf);

    // Recurse if nested object
    const initExpr = (propAssign as any).getInitializer?.();
    let nestedTree: TreeNode[] | {} = {};
    
    if (initExpr && initExpr.isKind && initExpr.isKind(SyntaxKind.ObjectLiteralExpression)) {
      const targetDecl =
        currentClass.getSourceFile().getProject().getSourceFiles()
          .flatMap(f => f.getClasses())
          .find(c => c.getName() === targetModel);
      if (targetDecl) {
        const nested = buildIncludeTreeFromObjectLiteral(initExpr, targetDecl, sf);
        nestedTree = nested.length > 0 ? nested : {};
      }
    }

    result.push({
      value: {
        name: relationProp.getName(),
        cidl_type: { Model: targetModel },
        nullable,
      },
      data_sources: nestedTree,
    });
  }

  return result;
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

      // fkPropName -> targetModelName (for scalar FK fields)
      const fkMap = new Map<string, string>();

      // relationPropName -> fkPropName (as *declared*, but we will alias-match)
      const oneToOneMap = new Map<string, string>();
      const oneToManyMap = new Map<string, string>();
      const manyToManyMap = new Map<string, string>();

      for (const prop of cls.getProperties()) {
        for (const dec of prop.getDecorators()) {
          const dname = getDecoratorPlainName(dec);
          if (dname === "ForeignKey") {
            // ForeignKey(Treat) or ForeignKey("Treat")
            const modelArg = getDecoratorArgText(dec, 0);
            if (modelArg) fkMap.set(prop.getName(), modelArg);
          } else if (dname === "OneToOne") {
            // OneToOne("treatId")  (may refer to treat_id)
            const fkNameArg = getDecoratorArgText(dec, 0);
            if (fkNameArg) oneToOneMap.set(prop.getName(), fkNameArg);
          } else if (dname === "OneToMany") {
            // OneToMany("dog_id") - stores the FK field name in the related model
            const fkNameArg = getDecoratorArgText(dec, 0);
            if (fkNameArg) oneToManyMap.set(prop.getName(), fkNameArg);
          } else if (dname === "ManyToMany") {
            // ManyToMany("StudentClasses") - stores the junction table name
            const junctionTableArg = getDecoratorArgText(dec, 0);
            if (junctionTableArg) manyToManyMap.set(prop.getName(), junctionTableArg);
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
            cidl_type: TypeCode.fromType(t, sf),
            nullable,
          },
          primary_key: hasDecoratorNamed(prop, "PrimaryKey"),
        });
        consumedProps.add(prop.getName());
      }

      // ---------- PASS 2a: OneToOne relations ----------
      for (const [relationPropName, fkName] of oneToOneMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        const fkProp = findClassPropertyByAlias(cls, fkName);
        if (!relationProp || !fkProp) {
          // If schema is malformed, fall back to default where possible
          if (relationProp && !consumedProps.has(relationPropName)) {
            pushDefaultAttribute(relationProp);
          }
          if (fkProp && !consumedProps.has(fkName)) {
            // NOTE: fkName may differ from fkProp.getName(); ensure we mark by real prop name if we emit default
            pushDefaultAttribute(fkProp);
          }
          continue;
        }
      
        // Determine target model
        let targetModel = fkMap.get(fkProp.getName());
        if (!targetModel) {
          const relT = relationProp.getType();
          targetModel = typeToModelName(relT, sf);
        }
        if (!targetModel) {
          // If still unknown, fall back to default props rather than crashing
          if (!consumedProps.has(relationPropName)) pushDefaultAttribute(relationProp);
          if (!consumedProps.has(fkProp.getName())) pushDefaultAttribute(fkProp);
          continue;
        }
      
        // Scalar FK entry (e.g., dogId: number)
        const fkType = fkProp.getType();
        const { nullable: fkNullable } = getNullability(fkProp, sf);
        attributes.push({
          foreign_key: { OneToOne: targetModel },
          value: {
            cidl_type: TypeCode.fromType(fkType, sf),
            name: fkProp.getName(),    // use actual FK prop name (e.g., "dogId")
            nullable: fkNullable,
          },
          primary_key: hasDecoratorNamed(fkProp, "PrimaryKey"),
        });
      
        // Relation entry (e.g., dog: Dog | undefined)
        const relNullable = getNullability(relationProp, sf).nullable;
        attributes.push({
          foreign_key: { OneToOne: targetModel },
          value: {
            cidl_type: { model: targetModel },  // Note: lowercase "model" to match expected output
            name: relationProp.getName(),  // relation property name, e.g. "dog"
            nullable: relNullable,
          },
          primary_key: false,
        });
      
        consumedProps.add(fkProp.getName());
        consumedProps.add(relationProp.getName());
      }

      // ---------- PASS 2a.1: Scalar FK fallbacks (ForeignKey without a matching OneToOne) ----------
      for (const [fkPropName, targetModel] of fkMap.entries()) {
        if (consumedProps.has(fkPropName)) continue; // already emitted via OneToOne
        const fkProp = findClassPropertyByAlias(cls, fkPropName);
        if (!fkProp) continue;
        const fkType = fkProp.getType();
        const { nullable: fkNullable } = getNullability(fkProp, sf);
        attributes.push({
          foreign_key: { OneToOne: targetModel },
          value: {
            cidl_type: TypeCode.fromType(fkType, sf),
            name: fkProp.getName(),
            nullable: fkNullable,
          },
          primary_key: hasDecoratorNamed(fkProp, "PrimaryKey"),
        });
        consumedProps.add(fkProp.getName());
      }

      // ---------- PASS 2a.2: OneToMany relations ----------
      for (const [relationPropName, _fkName] of oneToManyMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        if (!relationProp) continue;

        // Extract the model name from the array type
        const propType = relationProp.getType();
        const typeText = cleanTypeText(propType, sf);
        let targetModel: string | undefined;
        
        // Extract model name from array type (e.g., "Dog[]" -> "Dog")
        const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
        if (arrayMatch) {
          targetModel = arrayMatch[1];
        }
        
        if (!targetModel) {
          // Try to get from type symbol if text parsing fails
          const typeArgs = propType.getTypeArguments();
          if (typeArgs && typeArgs.length > 0) {
            targetModel = typeToModelName(typeArgs[0], sf);
          }
        }
        
        if (!targetModel) {
          console.log(`Warning: Could not determine target model for OneToMany relation ${relationProp.getName()}`);
          continue;
        }

        // Create the OneToMany relation entry with array type
        const { nullable } = getNullability(relationProp, sf);
        attributes.push({
          foreign_key: { OneToMany: targetModel },
          value: {
            cidl_type: { array: { model: targetModel } },
            name: relationProp.getName(),  // e.g., "dogs"
            nullable,
          },
          primary_key: false,
        });

        consumedProps.add(relationProp.getName());
      }

      // ---------- PASS 2a.3: ManyToMany relations ----------
      for (const [relationPropName, junctionTableName] of manyToManyMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        if (!relationProp) continue;

        // Extract the model name from the array type
        const propType = relationProp.getType();
        const typeText = cleanTypeText(propType, sf);
        let targetModel: string | undefined;
        
        // Extract model name from array type (e.g., "Class[]" -> "Class", "Student[]" -> "Student")
        const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
        if (arrayMatch) {
          targetModel = arrayMatch[1];
        }
        
        if (!targetModel) {
          // Try to get from type symbol if text parsing fails
          const typeArgs = propType.getTypeArguments();
          if (typeArgs && typeArgs.length > 0) {
            targetModel = typeToModelName(typeArgs[0], sf);
          }
        }
        
        if (!targetModel) {
          console.log(`Warning: Could not determine target model for ManyToMany relation ${relationProp.getName()}`);
          continue;
        }

        // Create the ManyToMany relation entry with array type
        // Note: The name field uses the junction table name, not the property name
        const { nullable } = getNullability(relationProp, sf);
        attributes.push({
          foreign_key: { ManyToMany: targetModel },
          value: {
            cidl_type: { array: { model: targetModel } },
            name: junctionTableName,  // e.g., "StudentClasses" - the junction table name
            nullable,
          },
          primary_key: false,
        });

        consumedProps.add(relationProp.getName());
      }
    
      // 2b) Emit any remaining properties (that weren't consumed by FK/OneToOne/OneToMany)
      // BUT SKIP properties that have @DataSource decorator
      for (const prop of cls.getProperties()) {
        if (consumedProps.has(prop.getName())) continue;
        
        // Skip properties with @DataSource decorator
        if (hasDecoratorNamed(prop, "DataSource")) continue;
        
        pushDefaultAttribute(prop);
      }

      // ---------- PASS 3: Data sources ----------
      const data_sources: any[] = [];
      for (const prop of cls.getProperties()) {
        const dsDec = prop.getDecorators().find(d => getDecoratorPlainName(d) === "DataSource");
        if (!dsDec) continue;

        const dsName = getDecoratorArgText(dsDec, 0) ?? prop.getName();
        const init = (prop as any).getInitializer?.();
        
        // Debug log
        console.log(`Found DataSource "${dsName}" on class ${className}`);
        
        if (!init || !init.isKind || !init.isKind(SyntaxKind.ObjectLiteralExpression)) {
          console.log(`  - No initializer or not object literal`);
          data_sources.push({ name: dsName, data_sources: {} });
          continue;
        }

        const tree = buildIncludeTreeFromObjectLiteral(init, cls, sf);
        console.log(`  - Built tree:`, JSON.stringify(tree, null, 2));
        data_sources.push({ name: dsName, data_sources: tree.length > 0 ? tree : {} });
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

          if (isD1DbType(pt, sf) || p.getName() === "db") continue;

          const nullable = getParamNullability(p, sf);
          parameters.push({
            name: p.getName(),
            cidl_type: TypeCode.fromType(pt, sf),
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

      // Get the source file path relative to the cwd
      const sourcePath = path.relative(cwd, sf.getFilePath());

      models.push({
        name: className,
        source_path: sourcePath,
        attributes,
        methods,
        ...(data_sources.length ? { data_sources } : {}),
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