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
  ClassDeclaration,
  Decorator,
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

const HTTP_VERBS = ["GET", "POST", "PUT", "PATCH", "DELETE"];

export enum TypeCode {
  Number = "Integer",
  String = "Text",
  Boolean = "Boolean",
  Date = "Date",
  D1Db = "D1Database",
  Response = "Response",
  Real = "Real",
  Blob = "Blob",
}

enum AttributeDecoratorKind {
  PrimaryKey = "PrimaryKey",
  ForeignKey = "ForeignKey",
  OneToOne = "OneToOne",
  OneToMany = "OneToMany",
  ManyToMany = "ManyToMany",
  DataSource = "DataSource",
}

export function extractModels(opts: ExtractOptions = {}) {
  // Determine top level CIDL values
  const cwd = opts.cwd ?? process.cwd();
  const { projectName: defaultName, version: defaultVersion } =
    readPackageJson(cwd);
  const projectName = opts.projectName ?? defaultName;
  const version = opts.version ?? defaultVersion;

  // Collect cloesce files
  const config = readCloesceConfig(cwd);
  const sourcePaths = Array.isArray(config.source)
    ? config.source
    : [config.source];
  const files = findCloesceFiles(cwd, sourcePaths);
  if (files.length === 0) {
    throw new Error(
      `No ".cloesce.ts" files found in specified source path(s): ${sourcePaths.join(", ")}`
    );
  }

  // Setup TypeScript project for AST traversal
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
  files.forEach((f) => project.addSourceFileAtPath(f));

  // Extract models
  const models = project.getSourceFiles().flatMap((sourceFile) =>
    sourceFile
      .getClasses()
      .filter((classDecl) => hasDecorator(classDecl, "D1"))
      .map((classDecl) => extract_model(classDecl, sourceFile))
  );

  return {
    version,
    project_name: projectName,
    language: "TypeScript",
    models,
  };
}

function extract_model(classDecl: ClassDeclaration, sourceFile: SourceFile) {
  const className = classDecl.getName() ?? "<anonymous>";
  const attributes: any[] = [];
  const navigationProperties: any[] = [];
  const dataSources: any[] = [];

  for (const prop of classDecl.getProperties()) {
    const decorators = prop.getDecorators();
    if (decorators.length === 0) {
      attributes.push({
        is_primary_key: false,
        foreign_key_reference: null,
        value: {
          name: prop.getName(),
          cidl_type: getTypeCode(prop.getType(), sourceFile),
          nullable: checkNullability(prop, sourceFile),
        },
      });
      continue;
    }

    // TODO: Limiting to one decorator. Can't get too fancy on us.
    const decorator = decorators[0];
    const name = getDecoratorName(decorator);
    switch (name) {
      case AttributeDecoratorKind.PrimaryKey: {
        attributes.push({
          is_primary_key: true,
          foreign_key_reference: null,
          value: {
            name: prop.getName(),
            cidl_type: getTypeCode(prop.getType(), sourceFile),
            nullable: checkNullability(prop, sourceFile),
          },
        });
        break;
      }
      case AttributeDecoratorKind.ForeignKey: {
        attributes.push({
          is_primary_key: false,
          foreign_key_reference: getDecoratorArgument(decorator, 0),
          value: {
            name: prop.getName(),
            cidl_type: getTypeCode(prop.getType(), sourceFile),
            nullable: checkNullability(prop, sourceFile),
          },
        });
        break;
      }
      case AttributeDecoratorKind.OneToOne: {
        const fkCls = extractModelName(prop.getType(), sourceFile);
        const reference = getDecoratorArgument(decorator, 0);
        if (!reference || !fkCls) return;
        navigationProperties.push({
          value: {
            name: prop.getName(),
            cidl_type: { Model: fkCls },
            nullable: checkNullability(prop, sourceFile),
          },
          kind: { [name]: { reference } },
        });
        break;
      }
      case AttributeDecoratorKind.OneToMany:
      case AttributeDecoratorKind.ManyToMany: {
        const fkCls = extractModelName(prop.getType(), sourceFile);
        const reference = getDecoratorArgument(decorator, 0);
        if (!reference || !fkCls) return;
        navigationProperties.push({
          value: {
            name: prop.getName(),
            cidl_type: { Array: { Model: fkCls } },
            nullable: checkNullability(prop, sourceFile),
          },
          kind: { [name]: { reference } },
        });
        break;
      }
      case AttributeDecoratorKind.DataSource: {
        const initializer = (prop as any).getInitializer?.();
        const tree = initializer
          ? buildIncludeTree(initializer, classDecl, sourceFile)
          : [];
        dataSources.push({ name: prop.getName(), tree });
        break;
      }
    }
  }

  const methods = classDecl
    .getMethods()
    .map((m) => extractMethod(m, sourceFile));

  return {
    name: className,
    attributes,
    navigation_properties: navigationProperties,
    methods,
    data_sources: dataSources,
    source_path: path.basename(sourceFile.getFilePath()),
  };
}

function cleanTypeText(t: Type, sf: SourceFile): string {
  return t.getText(sf).replace(/import\(".*?"\)\./g, "");
}

function readPackageJson(cwd: string) {
  const pkgPath = path.join(cwd, "package.json");
  let projectName = path.basename(cwd);
  let version = "0.0.1";

  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
    projectName = pkg.name ?? projectName;
    version = pkg.version ?? version;
  }

  return { projectName, version };
}

// We read the cloesce file, if there is no file or they messed up the syntax
// we should have some verbose errors
function readCloesceConfig(cwd: string): CloesceConfig {
  const configPath = path.join(cwd, "cloesce-config.json");

  if (!fs.existsSync(configPath)) {
    throw new Error(
      `No "cloesce-config.json" found in "${cwd}". Please create a cloesce-config.json with a "source" field.`
    );
  }

  try {
    const config = JSON.parse(
      fs.readFileSync(configPath, "utf8")
    ) as CloesceConfig;

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

function findCloesceFiles(root: string, searchPaths: string[]): string[] {
  const files: string[] = [];

  for (const searchPath of searchPaths) {
    const fullPath = path.resolve(root, searchPath);

    if (!fs.existsSync(fullPath)) {
      console.warn(
        `Warning: Path "${searchPath}" specified in cloesce-config.json does not exist`
      );
      continue;
    }

    const stats = fs.statSync(fullPath);

    if (stats.isFile() && /\.cloesce\.ts$/i.test(fullPath)) {
      files.push(fullPath);
    } else if (stats.isDirectory()) {
      files.push(...walkDirectory(fullPath));
    }
  }

  return files;
}

function walkDirectory(dir: string): string[] {
  const files: string[] = [];

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      files.push(...walkDirectory(fullPath));
    } else if (entry.isFile() && /\.cloesce\.ts$/i.test(entry.name)) {
      files.push(fullPath);
    }
  }

  return files;
}

function getDecoratorName(decorator: Decorator): string {
  const name = decorator.getName() ?? decorator.getExpression().getText();
  return String(name).replace(/\(.*\)$/, "");
}

function getDecoratorArgument(
  decorator: Decorator,
  index: number
): string | undefined {
  const args = decorator.getArguments();
  if (!args[index]) return undefined;

  const arg = args[index] as any;

  // Handle identifier
  if (arg.getKind?.() === SyntaxKind.Identifier) {
    return arg.getText();
  }

  // Handle string literal
  const text = arg.getText?.();
  if (!text) return undefined;

  const match = text.match(/^['"](.*)['"]$/);
  return match ? match[1] : text;
}

function hasDecorator(
  node: { getDecorators(): Decorator[] },
  name: string
): boolean {
  return node.getDecorators().some((d) => {
    const decoratorName = getDecoratorName(d);
    return decoratorName === name || decoratorName.endsWith("." + name);
  });
}

function getTypeCode(t: Type, sf: SourceFile): TypeCode {
  if (t.isNumber()) return TypeCode.Number;
  if (t.isString()) return TypeCode.String;
  if (t.isBoolean()) return TypeCode.Boolean;

  // Unwrap Promise types
  const symbol = t.getSymbol();
  if (symbol?.getName() === "Promise" && t.getTypeArguments().length === 1) {
    return getTypeCode(t.getTypeArguments()[0], sf);
  }

  const typeText = cleanTypeText(t, sf);

  const typeMap: Record<string, TypeCode> = {
    number: TypeCode.Number,
    string: TypeCode.String,
    boolean: TypeCode.Boolean,
    Date: TypeCode.Date,
    Response: TypeCode.Response,
    D1Db: TypeCode.D1Db,
    D1Database: TypeCode.D1Db,
  };

  return typeMap[typeText] ?? TypeCode.String;
}

function isD1DbType(t: Type, sf: SourceFile): boolean {
  const typeText = cleanTypeText(t, sf);
  const symbolName = t.getSymbol()?.getName();
  return (
    typeText === "D1Db" ||
    symbolName === "D1Db" ||
    getTypeCode(t, sf) === TypeCode.D1Db
  );
}

function extractModelName(t: Type, sf: SourceFile): string | undefined {
  const symbol = t.getSymbol();
  const name = symbol?.getName();

  if (name && !["Array", "Promise"].includes(name)) {
    return name;
  }

  const typeText = cleanTypeText(t, sf);
  const match = typeText.match(/^([A-Za-z_]\w*)/);
  return match?.[0];
}

function extractArrayModelName(t: Type, sf: SourceFile): string | undefined {
  const typeText = cleanTypeText(t, sf);
  const match = typeText.match(/^([A-Za-z_]\w*)\[\]/);
  return match?.[1];
}

function checkNullability(
  prop: PropertyDeclaration | PropertySignature,
  sf: SourceFile
): boolean {
  const type = prop.getType();

  // Check if union type contains null or undefined
  if (type.isUnion()) {
    for (const unionType of type.getUnionTypes()) {
      if (unionType.isNull() || unionType.isUndefined()) {
        return true;
      }
    }
  }

  // Check type text for null/undefined
  const typeNode = "getTypeNode" in prop ? prop.getTypeNode?.() : undefined;
  const typeText = typeNode?.getText();

  if (
    typeText &&
    (/\bnull\b/.test(typeText) || /\bundefined\b/.test(typeText))
  ) {
    return true;
  }

  return false;
}

function checkParameterNullability(param: ParameterDeclaration): boolean {
  if (param.hasQuestionToken()) return true;

  const type = param.getType();
  if (type.isUnion()) {
    for (const unionType of type.getUnionTypes()) {
      if (unionType.isNull() || unionType.isUndefined()) {
        return true;
      }
    }
  }

  const typeText = param.getTypeNode()?.getText();
  return typeText ? /\b(null|undefined)\b/.test(typeText) : false;
}

function findPropertyByName(
  cls: ClassDeclaration,
  name: string
): PropertyDeclaration | undefined {
  // Try exact match first
  const exactMatch = cls.getProperties().find((p) => p.getName() === name);
  if (exactMatch) return exactMatch;

  // Try normalized match (remove underscores and spaces, lowercase)
  const normalize = (s: string) => s.replace(/[_\s]/g, "").toLowerCase();
  const normalizedName = normalize(name);

  return cls
    .getProperties()
    .find((p) => normalize(p.getName()) === normalizedName);
}

function buildIncludeTree(
  obj: any,
  currentClass: ClassDeclaration,
  sf: SourceFile
): any[] {
  if (!obj.isKind || !obj.isKind(SyntaxKind.ObjectLiteralExpression)) {
    return [];
  }

  const result: any[] = [];

  for (const propAssign of obj.getProperties()) {
    if (!propAssign.isKind(SyntaxKind.PropertyAssignment)) continue;

    const relationKey = propAssign.getName();

    // Try to find the property with various naming conventions
    let relationProp =
      findPropertyByName(currentClass, relationKey) ||
      findPropertyByName(currentClass, relationKey + "s") ||
      (relationKey.endsWith("s")
        ? findPropertyByName(currentClass, relationKey.slice(0, -1))
        : undefined);

    if (!relationProp) {
      console.log(
        `  Warning: Could not find property "${relationKey}" in class ${currentClass.getName()}`
      );
      continue;
    }

    // Determine target model
    let targetModel: string | undefined;

    // Check decorators for model info
    for (const decorator of relationProp.getDecorators()) {
      const decoratorName = getDecoratorName(decorator);

      if (decoratorName === "OneToMany") {
        targetModel = extractArrayModelName(relationProp.getType(), sf);
        break;
      } else if (decoratorName === "ForeignKey") {
        targetModel = getDecoratorArgument(decorator, 0);
        break;
      }
    }

    if (!targetModel) {
      targetModel = extractModelName(relationProp.getType(), sf);
    }

    if (!targetModel) continue;

    const nullable = checkNullability(relationProp, sf);

    // Build TypedValue
    const typedValue = {
      name: relationProp.getName(),
      cidl_type: { Model: targetModel },
      nullable,
    };

    // Recurse for nested includes
    const initializer = (propAssign as any).getInitializer?.();
    let nestedTree: any[] = [];

    if (initializer?.isKind?.(SyntaxKind.ObjectLiteralExpression)) {
      const targetClass = currentClass
        .getSourceFile()
        .getProject()
        .getSourceFiles()
        .flatMap((f) => f.getClasses())
        .find((c) => c.getName() === targetModel);

      if (targetClass) {
        nestedTree = buildIncludeTree(initializer, targetClass, sf);
      }
    }

    result.push([typedValue, nestedTree]);
  }

  return result;
}

function extractMethod(method: MethodDeclaration, sf: SourceFile): any {
  const decorators = method.getDecorators();
  const decoratorNames = decorators.map((d) => getDecoratorName(d));

  const httpVerb =
    HTTP_VERBS.find((verb) => decoratorNames.includes(verb)) || null;

  const parameters: any[] = [];

  for (const param of method.getParameters()) {
    const paramType = param.getType();
    const typeText = cleanTypeText(paramType, sf);

    // Skip Request and D1Db parameters
    if (
      typeText === "Request" ||
      isD1DbType(paramType, sf) ||
      param.getName() === "db"
    ) {
      continue;
    }

    parameters.push({
      name: param.getName(),
      cidl_type: getTypeCode(paramType, sf),
      nullable: checkParameterNullability(param),
    });
  }

  // Extract return type
  const returnType = method.getReturnType();
  let return_type = null;

  if (returnType && !returnType.isVoid()) {
    const typeText = cleanTypeText(returnType, sf);
    if (typeText !== "void" && typeText !== "Promise<void>") {
      const modelName = extractModelName(returnType, sf);
      return_type = modelName
        ? { Model: modelName }
        : getTypeCode(returnType, sf);
    }
  }

  return {
    name: method.getName(),
    is_static: method.isStatic(),
    http_verb: httpVerb,
    return_type,
    parameters,
  };
}
