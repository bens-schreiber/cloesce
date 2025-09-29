import {
  Project,
  Type,
  SourceFile,
  PropertyDeclaration,
  MethodDeclaration,
  SyntaxKind,
  ClassDeclaration,
  Decorator,
  Expression,
} from "ts-morph";

import {
  CidlIncludeTree,
  CidlSpec,
  CidlType,
  DataSource,
  Either,
  HttpVerb,
  Model,
  ModelAttribute,
  ModelMethod,
  NamedTypedValue,
  NavigationProperty,
  WranglerEnv,
  left,
  right,
} from "./common.js";

enum AttributeDecoratorKind {
  PrimaryKey = "PrimaryKey",
  ForeignKey = "ForeignKey",
  OneToOne = "OneToOne",
  OneToMany = "OneToMany",
  ManyToMany = "ManyToMany",
  DataSource = "DataSource",
}

enum ClassDecoratorKind {
  D1 = "D1",
  WranglerEnv = "WranglerEnv",
}

enum ParameterDecoratorKind {
  Inject = "Inject",
}

export class CidlExtractor {
  constructor(
    public projectName: string,
    public version: string,
  ) {}

  extract(project: Project): Either<string, CidlSpec> {
    let models = [];
    for (const sourceFile of project.getSourceFiles()) {
      for (const classDecl of sourceFile.getClasses()) {
        if (!hasDecorator(classDecl, ClassDecoratorKind.D1)) continue;

        const result = CidlExtractor.model(classDecl, sourceFile);
        if (!result.ok) {
          return left(result.value);
        }
        models.push(result.value);
      }
    }

    const wranglerEnvs: WranglerEnv[] = project
      .getSourceFiles()
      .flatMap((sourceFile) => {
        return sourceFile
          .getClasses()
          .filter((classDecl) =>
            hasDecorator(classDecl, ClassDecoratorKind.WranglerEnv),
          )
          .map((classDecl) => {
            return {
              name: classDecl.getName(),
              source_path: sourceFile.getFilePath().toString(),
            } as WranglerEnv;
          });
      });

    if (wranglerEnvs.length < 1) {
      left("Missing wrangler environment @WranglerEnv");
    }
    if (wranglerEnvs.length > 1) {
      // todo: err
    }

    return right({
      version: this.version,
      project_name: this.projectName,
      language: "TypeScript",
      wrangler_env: wranglerEnvs[0],
      models,
    });
  }

  private static model(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<string, Model> {
    const name = classDecl.getName() ?? "<anonymous>";
    const attributes: ModelAttribute[] = [];
    const navigationProperties: NavigationProperty[] = [];
    const dataSources: DataSource[] = [];
    const methods: ModelMethod[] = [];
    let primary_key: NamedTypedValue | undefined = undefined;

    for (const prop of classDecl.getProperties()) {
      const decorators = prop.getDecorators();

      // No decorators means this is a standard attribute
      if (decorators.length === 0) {
        let typeRes = CidlExtractor.cidlType(prop.getType());
        if (!typeRes.ok) {
          return typeRes;
        }

        let [cidl_type, nullable] = typeRes.value;
        attributes.push({
          foreign_key_reference: null,
          value: {
            name: prop.getName(),
            cidl_type,
            nullable,
          },
        });
        continue;
      }

      // TODO: Limiting to one decorator. Can't get too fancy on us.
      const decorator = decorators[0];
      const name = getDecoratorName(decorator);

      let typeRes = CidlExtractor.cidlType(prop.getType());
      if (!typeRes.ok) {
        return typeRes;
      }

      // Process decorators
      let [cidl_type, nullable] = typeRes.value;
      switch (name) {
        case AttributeDecoratorKind.PrimaryKey: {
          primary_key = {
            name: prop.getName(),
            cidl_type,
            nullable,
          };
          break;
        }
        case AttributeDecoratorKind.ForeignKey: {
          attributes.push({
            foreign_key_reference: getDecoratorArgument(decorator, 0) ?? null,
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
          });
          break;
        }
        case AttributeDecoratorKind.OneToOne: {
          const reference = getDecoratorArgument(decorator, 0);
          if (!reference) {
            return left(
              `One to One navigation properties must hold a model type ${name}.${prop.getName()}`,
            );
          }

          navigationProperties.push({
            value: { name: prop.getName(), cidl_type, nullable },
            kind: { OneToOne: { reference } },
          });
          break;
        }
        case AttributeDecoratorKind.OneToMany: {
          const reference = getDecoratorArgument(decorator, 0);
          if (!reference) {
            return left(
              `One to Many navigation properties must hold an attribute reference ${name}.${prop.getName()}`,
            );
          }

          navigationProperties.push({
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
            kind: { OneToMany: { reference } },
          });
          break;
        }
        case AttributeDecoratorKind.ManyToMany: {
          const unique_id = getDecoratorArgument(decorator, 0);
          if (!unique_id)
            return left(
              `Many to Many navigation properties must hold a unique table ID: ${name}.${prop.getName()}`,
            );
          navigationProperties.push({
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
            kind: { ManyToMany: { unique_id } },
          });
          break;
        }
        case AttributeDecoratorKind.DataSource: {
          const initializer = prop.getInitializer();
          if (!initializer) {
            return left(
              `Invalid Data Source initializer on model ${name}.${prop.getName()}`,
            );
          }

          let treeRes = CidlExtractor.includeTree(
            initializer,
            classDecl,
            sourceFile,
          );
          if (!treeRes.ok) {
            return treeRes;
          }

          dataSources.push({ name: prop.getName(), tree: treeRes.value });
          break;
        }
      }
    }

    if (primary_key == undefined) {
      return left(`Missing primary key: ${name}`);
    }

    // Process methods
    for (const m of classDecl.getMethods()) {
      const result = CidlExtractor.method(m);
      if (!result.ok) {
        return left(result.value);
      }
      methods.push(result.value);
    }

    return right({
      name,
      attributes,
      primary_key,
      navigation_properties: navigationProperties,
      methods,
      data_sources: dataSources,
      source_path: sourceFile.getFilePath().toString(),
    });
  }

  /// Returns a `CidlType` from a TypeScript type, along with if the base value is nullable.
  /// Throws an error if no type can be extracted.
  private static cidlType(
    type: Type,
    inject: boolean = false,
  ): Either<string, [CidlType, boolean]> {
    let map: Record<string, CidlType> = {
      number: "Integer", // TODO: It's wrong to assume number is always an int.
      Number: "Integer",
      string: "Text",
      String: "Text",
      boolean: "Integer",
      Boolean: "Integer",
      Date: "Text",
    };

    // TODO: We don't support type unions like Foo | Bar, should we?
    let nullable = type.getUnionTypes().find((t) => t.isNull()) !== undefined;

    // Split by generics and imports
    let split = type.getText().split(/<|>|import\([^)]+\)\.?/);

    let cidlType = split.reduceRight<CidlType | undefined>((acc, x) => {
      // Strip unions
      let base = x
        .split("|")
        .map((s) => s.trim())
        .find((s) => s !== "null" && s !== "undefined")!;

      // Disregard any promises, they have no meaning as of now
      if (!base || base === "Promise") return acc!;

      // Primitive or nullable primitive
      if (map[base] !== undefined) return map[base];

      // Array of primitive
      if (base.endsWith("[]")) {
        const item = base.slice(0, -2);
        return map[item] !== undefined
          ? { Array: map[item] }
          : { Array: { Model: item } };
      }

      // Skip void
      if (base == "void") return acc;

      // Result wrapper
      if (base === "HttpResult") {
        return { HttpResult: acc == undefined ? null : acc };
      }

      // Inject wrapper
      if (inject) {
        return { Inject: base };
      }

      // Model wrapper
      return { Model: base };
    }, undefined);

    if (cidlType === undefined) {
      left(`Unknown or unsupported type ${type.getText()}`);
    }

    return right([cidlType!, nullable]);
  }

  // TODO: Should really be more descriptive with the errors here
  private static includeTree(
    expr: Expression,
    currentClass: ClassDeclaration,
    sf: SourceFile,
  ): Either<string, CidlIncludeTree> {
    if (!expr.isKind || !expr.isKind(SyntaxKind.ObjectLiteralExpression)) {
      return left(`Invalid include tree.`);
    }

    const result: CidlIncludeTree = [];
    for (const prop of expr.getProperties()) {
      if (!prop.isKind(SyntaxKind.PropertyAssignment)) continue;

      let navProp = findPropertyByName(currentClass, prop.getName());
      if (!navProp) {
        console.log(
          `  Warning: Could not find property "${prop.getName()}" in class ${currentClass.getName()}`,
        );
        continue;
      }

      let typeRes = CidlExtractor.cidlType(navProp.getType());
      if (!typeRes.ok) {
        return typeRes;
      }

      let [cidl_type, _] = typeRes.value;
      if (typeof cidl_type === "string") {
        return left(`Invalid include tree type ${cidl_type} expected Model`);
      }

      const typedValue = {
        name: navProp.getName(),
        cidl_type,
        nullable: false, // TODO: hardcoding this for now, it doesn't mean anything for the IncludeTree
      };

      // Recurse for nested includes
      const initializer = (prop as any).getInitializer?.();
      let nestedTree: CidlIncludeTree = [];

      if (initializer?.isKind?.(SyntaxKind.ObjectLiteralExpression)) {
        let targetModel = getModelName(cidl_type);
        const targetClass = currentClass
          .getSourceFile()
          .getProject()
          .getSourceFiles()
          .flatMap((f) => f.getClasses())
          .find((c) => c.getName() === targetModel);

        if (targetClass) {
          let treeRes = CidlExtractor.includeTree(initializer, targetClass, sf);
          if (!treeRes.ok) {
            return treeRes;
          }
          nestedTree = treeRes.value;
        }
      }

      result.push([typedValue, nestedTree]);
    }

    return right(result);
  }

  private static method(
    method: MethodDeclaration,
  ): Either<string, ModelMethod> {
    const decorators = method.getDecorators();
    const decoratorNames = decorators.map((d) => getDecoratorName(d));

    const httpVerb = decoratorNames.find((name) =>
      Object.values(HttpVerb).includes(name as HttpVerb),
    ) as HttpVerb;

    const parameters: NamedTypedValue[] = [];

    for (const param of method.getParameters()) {
      // Handle injected param
      if (param.getDecorator(ParameterDecoratorKind.Inject)) {
        let typeRes = CidlExtractor.cidlType(param.getType(), true);
        if (!typeRes.ok) {
          return typeRes;
        }
        let [cidl_type, nullable] = typeRes.value;

        parameters.push({
          name: param.getName(),
          cidl_type: cidl_type,
          nullable: false,
        });
        continue;
      }

      // Handle all other params
      let typeRes = CidlExtractor.cidlType(param.getType());
      if (!typeRes.ok) {
        return typeRes;
      }

      let [cidl_type, nullable] = typeRes.value;
      parameters.push({
        name: param.getName(),
        cidl_type,
        nullable,
      });
    }

    // TODO: return types cant be nullable??
    let typeRes = CidlExtractor.cidlType(method.getReturnType());
    if (!typeRes.ok) {
      return typeRes;
    }
    let [return_type, _] = typeRes.value;

    return right({
      name: method.getName(),
      is_static: method.isStatic(),
      http_verb: httpVerb,
      return_type,
      parameters,
    });
  }
}

function getDecoratorName(decorator: Decorator): string {
  const name = decorator.getName() ?? decorator.getExpression().getText();
  return String(name).replace(/\(.*\)$/, "");
}

function getDecoratorArgument(
  decorator: Decorator,
  index: number,
): string | undefined {
  const args = decorator.getArguments();
  if (!args[index]) return undefined;

  const arg = args[index] as any;

  // Identifier
  if (arg.getKind?.() === SyntaxKind.Identifier) {
    return arg.getText();
  }

  // String literal
  const text = arg.getText?.();
  if (!text) return undefined;

  const match = text.match(/^['"](.*)['"]$/);
  return match ? match[1] : text;
}

function getModelName(t: CidlType): string | undefined {
  if (typeof t === "string") return undefined;

  if ("Model" in t) {
    return t.Model;
  } else if ("Array" in t) {
    return getModelName(t.Array);
  } else if ("HttpResult" in t) {
    if (t == null) return undefined;
    return getModelName(t.HttpResult!);
  }

  return undefined;
}

function findPropertyByName(
  cls: ClassDeclaration,
  name: string,
): PropertyDeclaration | undefined {
  const exactMatch = cls.getProperties().find((p) => p.getName() === name);
  return exactMatch;
}

function hasDecorator(
  node: { getDecorators(): Decorator[] },
  name: string,
): boolean {
  return node.getDecorators().some((d) => {
    const decoratorName = getDecoratorName(d);
    return decoratorName === name || decoratorName.endsWith("." + name);
  });
}
