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

// ... [keeping all the helper functions unchanged until buildIncludeTreeFromObjectLiteral] ...

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

  if ((a as any).getKind && (a as any).getKind() === 77 /* SyntaxKind.Identifier */) {
    return (a as any).getText();
  }

  const txt = (a as any).getText?.();
  if (!txt) return undefined;
  const m = txt.match(/^['"](.*)['"]$/);
  return m ? m[1] : txt;
}

function typeToModelName(t: Type, sf: SourceFile): string | undefined {
  const sym = t.getSymbol();
  const name = sym?.getName();
  if (name && !["Array", "Promise"].includes(name)) return name;

  const txt = cleanTypeText(t, sf);
  const m = txt.match(/^[A-Za-z_]\w*/);
  return m ? m[0] : undefined;
}

function normalizeIdent(s: string): string {
  return s.replace(/[_\s]/g, "").toLowerCase();
}

function findClassPropertyByAlias(
  cls: import("ts-morph").ClassDeclaration,
  wanted: string,
) {
  const direct = cls.getProperties().find(p => p.getName() === wanted);
  if (direct) return direct;

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
    if (/\.cloesce\.ts$/i.test(fullPath)) {
      out.push(fullPath);
    }
  } else if (stats.isDirectory()) {
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

function cleanTypeText(t: Type, sf: SourceFile) {
  return t.getText(sf).replace(/import\(".*?"\)\./g, "");
}

export enum TypeCode {
  Number = "Integer",
  String = "Text",
  Boolean = "Boolean",
  Date = "Date",
  D1Db = "D1Database",  // Changed from "D1Db" to match Rust enum
  Response = "Response",
  Real = "Real",
  Blob = "Blob",
}

export namespace TypeCode {
  const basicTypeMap: Record<string, TypeCode> = {
    number: TypeCode.Number,
    string: TypeCode.String,
    boolean: TypeCode.Boolean,
    Date: TypeCode.Date,
    Response: TypeCode.Response,
    D1Db: TypeCode.D1Db,
    D1Database: TypeCode.D1Db,
  };

  export function fromType(t: Type, sf: SourceFile): TypeCode {
    const txt = cleanTypeText(t, sf);
    const symName = t.getSymbol()?.getName();

    if (t.isNumber()) return TypeCode.Number;
    if (t.isString()) return TypeCode.String;
    if (t.isBoolean()) return TypeCode.Boolean;

    if (symName === "Promise" && t.getTypeArguments().length === 1) {
      return fromType(t.getTypeArguments()[0], sf);
    }

    if (txt in basicTypeMap)
      return basicTypeMap[txt as keyof typeof basicTypeMap];

    return TypeCode.String;
  }
}

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

function typeNodeTextHasNullish(typeText?: string): {
  hasNull: boolean;
  hasUndefined: boolean;
} {
  if (!typeText) return { hasNull: false, hasUndefined: false };
  const hasNull = /\bnull\b/.test(typeText);
  const hasUndefined = /\bundefined\b/.test(typeText);
  return { hasNull, hasUndefined };
}

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
  if (
    (prop as PropertyDeclaration).hasQuestionToken &&
    (prop as PropertyDeclaration).hasQuestionToken()
  ) {
    return { nullable: true, reason: "optional" };
  }

  const t = (prop as PropertyDeclaration).getType
    ? (prop as PropertyDeclaration).getType()
    : (prop as PropertySignature).getType();
  const { hasNull, hasUndefined } = unionIncludesNullish(t);
  if (hasNull) return { nullable: true, reason: "union-null" };
  if (hasUndefined) return { nullable: true, reason: "union-undefined" };

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

function getParamNullability(
  param: ParameterDeclaration,
  sf: SourceFile,
): boolean {
  if (param.hasQuestionToken()) return true;

  const t = param.getType();

  const { hasNull, hasUndefined } = unionIncludesNullish(t);
  if (hasNull || hasUndefined) return true;

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
    const plain = String(n).replace(/\(.*\)$/, "");
    return plain === name || plain.endsWith("." + name);
  });
}

// Fixed helper for data sources to match Rust's IncludeTree structure
function buildIncludeTreeFromObjectLiteral(
  obj: import("ts-morph").ObjectLiteralExpression,
  currentClass: import("ts-morph").ClassDeclaration,
  sf: SourceFile,
): any[] {  // Returns array of tuples [TypedValue, IncludeTree]
  const result: any[] = [];

  for (const propAssign of obj.getProperties()) {
    if (!propAssign.isKind(SyntaxKind.PropertyAssignment)) continue;

    const relationKey = propAssign.getName();
    let relationProp = findClassPropertyByAlias(currentClass, relationKey);
    
    if (!relationProp && !relationKey.endsWith('s')) {
      relationProp = findClassPropertyByAlias(currentClass, relationKey + 's');
    }
    if (!relationProp && relationKey.endsWith('s')) {
      relationProp = findClassPropertyByAlias(currentClass, relationKey.slice(0, -1));
    }
    
    if (!relationProp) {
      console.log(`  Warning: Could not find property "${relationKey}" in class ${currentClass.getName()}`);
      continue;
    }

    let targetModel: string | undefined;
    
    const oneToManyDec = relationProp.getDecorators().find(d => getDecoratorPlainName(d) === "OneToMany");
    if (oneToManyDec) {
      const propType = relationProp.getType();
      const typeText = cleanTypeText(propType, sf);
      const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
      if (arrayMatch) {
        targetModel = arrayMatch[1];
      }
    }
    
    if (!targetModel) {
      const fkDec = relationProp.getDecorators().find(d => getDecoratorPlainName(d) === "ForeignKey");
      targetModel = fkDec ? getDecoratorArgText(fkDec, 0) : undefined;
    }
    
    if (!targetModel) {
      targetModel = typeToModelName(relationProp.getType(), sf);
    }
    
    if (!targetModel) continue;

    const { nullable } = getNullability(relationProp, sf);

    // Build TypedValue matching Rust structure
    const typedValue = {
      name: relationProp.getName(),
      cidl_type: { Model: targetModel },  // Fixed: Model with capital M
      nullable,
    };

    // Recurse if nested object
    const initExpr = (propAssign as any).getInitializer?.();
    let nestedTree: any[] = [];
    
    if (initExpr && initExpr.isKind && initExpr.isKind(SyntaxKind.ObjectLiteralExpression)) {
      const targetDecl =
        currentClass.getSourceFile().getProject().getSourceFiles()
          .flatMap(f => f.getClasses())
          .find(c => c.getName() === targetModel);
      if (targetDecl) {
        nestedTree = buildIncludeTreeFromObjectLiteral(initExpr, targetDecl, sf);
      }
    }

    // Push as tuple [TypedValue, IncludeTree]
    result.push([typedValue, nestedTree]);
  }

  return result;
}

// Main extraction function
export function extractModels(opts: ExtractOptions = {}) {
  const cwd = opts.cwd ?? process.cwd();
  const { projectName: pn, version: ver } = readPkgMeta(cwd);
  const projectName = opts.projectName ?? pn;
  const version = opts.version ?? ver;

  const config = readCloesceConfig(cwd);
  if (!config) {
    throw new Error(
      `No "cloesce-config.json" found in "${cwd}". Please create a cloesce-config.json with a "source" field.`,
    );
  }

  const sourcePaths = Array.isArray(config.source)
    ? config.source
    : [config.source];

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
      if (!hasDecoratorNamed(cls, "D1")) continue;

      const className = cls.getName() ?? "<anonymous>";

      const fkMap = new Map<string, string>();
      const oneToOneMap = new Map<string, string>();
      const oneToManyMap = new Map<string, string>();
      const manyToManyMap = new Map<string, string>();

      for (const prop of cls.getProperties()) {
        for (const dec of prop.getDecorators()) {
          const dname = getDecoratorPlainName(dec);
          if (dname === "ForeignKey") {
            const modelArg = getDecoratorArgText(dec, 0);
            if (modelArg) fkMap.set(prop.getName(), modelArg);
          } else if (dname === "OneToOne") {
            const fkNameArg = getDecoratorArgText(dec, 0);
            if (fkNameArg) oneToOneMap.set(prop.getName(), fkNameArg);
          } else if (dname === "OneToMany") {
            const fkNameArg = getDecoratorArgText(dec, 0);
            if (fkNameArg) oneToManyMap.set(prop.getName(), fkNameArg);
          } else if (dname === "ManyToMany") {
            const junctionTableArg = getDecoratorArgText(dec, 0);
            if (junctionTableArg) manyToManyMap.set(prop.getName(), junctionTableArg);
          }
        }
      }

      const attributes: any[] = [];
      const consumedProps = new Set<string>();
    
      function pushDefaultAttribute(prop: PropertyDeclaration) {
        const t = prop.getType();
        const { nullable } = getNullability(prop, sf);
        
        // Check if this property has any relationship decorators
        let foreignKey: string | null = null;
        let cidlType: any = TypeCode.fromType(t, sf);
        
        // Check for ForeignKey decorator
        const fkDec = prop.getDecorators().find(d => getDecoratorPlainName(d) === "ForeignKey");
        if (fkDec) {
          foreignKey = getDecoratorArgText(fkDec, 0) || null;
        }
        
        // Check for OneToMany decorator
        const oneToManyDec = prop.getDecorators().find(d => getDecoratorPlainName(d) === "OneToMany");
        if (oneToManyDec) {
          // Extract model from array type
          const typeText = cleanTypeText(t, sf);
          const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
          if (arrayMatch) {
            foreignKey = arrayMatch[1];
            cidlType = { Array: { Model: arrayMatch[1] } };
          }
        }
        
        // Check for ManyToMany decorator
        const manyToManyDec = prop.getDecorators().find(d => getDecoratorPlainName(d) === "ManyToMany");
        if (manyToManyDec) {
          // Extract model from array type
          const typeText = cleanTypeText(t, sf);
          const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
          if (arrayMatch) {
            foreignKey = arrayMatch[1];
            cidlType = { Array: { Model: arrayMatch[1] } };
          }
        }
        
        attributes.push({
          primary_key: hasDecoratorNamed(prop, "PrimaryKey"),
          foreign_key: foreignKey,
          value: {
            name: prop.getName(),
            cidl_type: cidlType,
            nullable,
          },
        });
        consumedProps.add(prop.getName());
      }

      // OneToOne relations
      for (const [relationPropName, fkName] of oneToOneMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        const fkProp = findClassPropertyByAlias(cls, fkName);
        if (!relationProp || !fkProp) {
          if (relationProp && !consumedProps.has(relationPropName)) {
            pushDefaultAttribute(relationProp);
          }
          if (fkProp && !consumedProps.has(fkName)) {
            pushDefaultAttribute(fkProp);
          }
          continue;
        }
      
        let targetModel = fkMap.get(fkProp.getName());
        if (!targetModel) {
          const relT = relationProp.getType();
          targetModel = typeToModelName(relT, sf);
        }
        if (!targetModel) {
          if (!consumedProps.has(relationPropName)) pushDefaultAttribute(relationProp);
          if (!consumedProps.has(fkProp.getName())) pushDefaultAttribute(fkProp);
          continue;
        }
      
        // Scalar FK entry
        const fkType = fkProp.getType();
        const { nullable: fkNullable } = getNullability(fkProp, sf);
        attributes.push({
          primary_key: hasDecoratorNamed(fkProp, "PrimaryKey"),
          foreign_key: targetModel,  // Fixed: just the model name string
          value: {
            cidl_type: TypeCode.fromType(fkType, sf),
            name: fkProp.getName(),
            nullable: fkNullable,
          },
        });
      
        // Relation entry - Fixed: use Model enum variant
        const relNullable = getNullability(relationProp, sf).nullable;
        attributes.push({
          primary_key: false,
          foreign_key: targetModel,  // Fixed: just the model name string
          value: {
            cidl_type: { Model: targetModel },  // Fixed: uppercase Model
            name: relationProp.getName(),
            nullable: relNullable,
          },
        });
      
        consumedProps.add(fkProp.getName());
        consumedProps.add(relationProp.getName());
      }

      // Scalar FK fallbacks
      for (const [fkPropName, targetModel] of fkMap.entries()) {
        if (consumedProps.has(fkPropName)) continue;
        const fkProp = findClassPropertyByAlias(cls, fkPropName);
        if (!fkProp) continue;
        const fkType = fkProp.getType();
        const { nullable: fkNullable } = getNullability(fkProp, sf);
        attributes.push({
          primary_key: hasDecoratorNamed(fkProp, "PrimaryKey"),
          foreign_key: targetModel,  // Fixed: just the model name string
          value: {
            cidl_type: TypeCode.fromType(fkType, sf),
            name: fkProp.getName(),
            nullable: fkNullable,
          },
        });
        consumedProps.add(fkProp.getName());
      }

      // Remaining properties (skip DataSource)
      for (const prop of cls.getProperties()) {
        if (consumedProps.has(prop.getName())) continue;
        if (hasDecoratorNamed(prop, "DataSource")) continue;
        pushDefaultAttribute(prop);
      }

      // Navigation properties - Fixed structure
      const navigation_properties: any[] = [];
      
      // OneToOne navigation
      for (const [relationPropName, _fkName] of oneToOneMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        if (!relationProp) continue;
        
        let targetModel = typeToModelName(relationProp.getType(), sf);
        if (!targetModel) continue;
        
        const { nullable } = getNullability(relationProp, sf);
        navigation_properties.push({
          value: {
            name: relationProp.getName(),
            cidl_type: { Model: targetModel },
            nullable
          },
          foreign_key: { OneToOne: { reference: targetModel } }  // Fixed structure
        });
      }
      
      // OneToMany navigation
      for (const [relationPropName, _fkName] of oneToManyMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        if (!relationProp) continue;

        const propType = relationProp.getType();
        const typeText = cleanTypeText(propType, sf);
        let targetModel: string | undefined;
        
        const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
        if (arrayMatch) {
          targetModel = arrayMatch[1];
        }
        
        if (!targetModel) continue;

        const { nullable } = getNullability(relationProp, sf);
        navigation_properties.push({
          value: {
            name: relationProp.getName(),
            cidl_type: { Array: { Model: targetModel } },  // Fixed: nested structure
            nullable
          },
          foreign_key: { OneToMany: { reference: targetModel } }  // Fixed structure
        });
      }

      // ManyToMany navigation
      for (const [relationPropName, junctionTableName] of manyToManyMap.entries()) {
        const relationProp = findClassPropertyByAlias(cls, relationPropName);
        if (!relationProp) continue;

        const propType = relationProp.getType();
        const typeText = cleanTypeText(propType, sf);
        let targetModel: string | undefined;
        
        const arrayMatch = typeText.match(/^([A-Za-z_]\w*)\[\]/);
        if (arrayMatch) {
          targetModel = arrayMatch[1];
        }
        
        if (!targetModel) continue;

        const { nullable } = getNullability(relationProp, sf);
        navigation_properties.push({
          value: {
            name: relationProp.getName(),
            cidl_type: { Array: { Model: targetModel } },  // Fixed: nested structure
            nullable
          },
          foreign_key: { ManyToMany: { unique_id: junctionTableName } }  // Fixed structure
        });
      }

      // Data sources - Fixed structure
      const data_sources: any[] = [];
      for (const prop of cls.getProperties()) {
        const dsDec = prop.getDecorators().find(d => getDecoratorPlainName(d) === "DataSource");
        if (!dsDec) continue;

        const dsName = getDecoratorArgText(dsDec, 0) ?? prop.getName();
        const init = (prop as any).getInitializer?.();
        
        console.log(`Found DataSource "${dsName}" on class ${className}`);
        
        if (!init || !init.isKind || !init.isKind(SyntaxKind.ObjectLiteralExpression)) {
          console.log(`  - No initializer or not object literal`);
          data_sources.push({ name: dsName, tree: [] });  // Fixed: tree is array
          continue;
        }

        const tree = buildIncludeTreeFromObjectLiteral(init, cls, sf);
        console.log(`  - Built tree:`, JSON.stringify(tree, null, 2));
        data_sources.push({ name: dsName, tree });  // Fixed: use 'tree' field
      }

      // Methods
      const methods = cls.getMethods().map((m) => {
        const decos = m
          .getDecorators()
          .map((d) => d.getName() ?? d.getExpression().getText());
        const HTTP_VERBS = ["GET", "POST", "PUT", "PATCH", "DELETE"];
        const httpVerb =
          HTTP_VERBS.find((verb) => decos.includes(verb)) || null;

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

        // Get return type if possible
        const returnType = m.getReturnType();
        let return_type = null;
        if (returnType && !returnType.isVoid()) {
          const typeText = cleanTypeText(returnType, sf);
          if (typeText !== "void" && typeText !== "Promise<void>") {
            // Try to determine if it's a model or primitive type
            const modelName = typeToModelName(returnType, sf);
            if (modelName) {
              return_type = { Model: modelName };
            } else {
              return_type = TypeCode.fromType(returnType, sf);
            }
          }
        }

        return {
          name: m.getName(),
          is_static: m.isStatic(),
          http_verb: httpVerb,
          return_type,
          parameters,
        };
      });

      const sourcePath = path.basename(sf.getFilePath());

      models.push({
        name: className,
        attributes,
        navigation_properties,
        methods,
        data_sources,
        source_path: sourcePath,
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