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
  HttpVerb,
  Model,
  ModelAttribute,
  ModelMethod,
  NamedTypedValue,
  NavigationProperty,
  WranglerEnv,
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

  public extract(project: Project): CidlSpec {
    const models: Model[] = project.getSourceFiles().flatMap((sourceFile) => {
      return sourceFile
        .getClasses()
        .filter((classDecl) => hasDecorator(classDecl, ClassDecoratorKind.D1))
        .flatMap((classDecl) => {
          const model = CidlExtractor.model(classDecl, sourceFile);
          return model ? [model] : [];
        });
    });

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
      // todo: err
    }
    if (wranglerEnvs.length > 1) {
      // todo: err
    }

    return {
      version: this.version,
      project_name: this.projectName,
      language: "TypeScript",
      wrangler_env: wranglerEnvs[0],
      models,
    };
  }

  private static model(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Model | undefined {
    const className = classDecl.getName() ?? "<anonymous>";
    const attributes: ModelAttribute[] = [];
    const navigationProperties: NavigationProperty[] = [];
    const dataSources: DataSource[] = [];

    for (const prop of classDecl.getProperties()) {
      const decorators = prop.getDecorators();

      // No decorators means this is a standard attribute
      if (decorators.length === 0) {
        let [cidl_type, nullable] = CidlExtractor.cidlType(prop.getType());
        attributes.push({
          is_primary_key: false,
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
      let [cidl_type, nullable] = CidlExtractor.cidlType(prop.getType());
      switch (name) {
        case AttributeDecoratorKind.PrimaryKey: {
          attributes.push({
            is_primary_key: true,
            foreign_key_reference: null,
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
          });
          break;
        }
        case AttributeDecoratorKind.ForeignKey: {
          attributes.push({
            is_primary_key: false,
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
          if (!reference) return;

          navigationProperties.push({
            value: { name: prop.getName(), cidl_type, nullable },
            kind: { OneToOne: { reference } },
          });
          break;
        }
        case AttributeDecoratorKind.OneToMany:
          const reference = getDecoratorArgument(decorator, 0);
          if (!reference) return;

          navigationProperties.push({
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
            kind: { OneToMany: { reference } },
          });
          break;
        case AttributeDecoratorKind.ManyToMany: {
          const unique_id = getDecoratorArgument(decorator, 0);
          if (!unique_id) return;
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
          const initializer = prop.getInitializer?.();
          const tree = initializer
            ? CidlExtractor.includeTree(initializer, classDecl, sourceFile)
            : [];
          dataSources.push({ name: prop.getName(), tree });
          break;
        }
      }
    }

    const methods = classDecl.getMethods().map((m) => CidlExtractor.method(m));

    return {
      name: className,
      attributes,
      navigation_properties: navigationProperties,
      methods,
      data_sources: dataSources,
      source_path: sourceFile.getFilePath().toString(),
    };
  }

  /// Returns a `CidlType` from a TypeScript type, along with if the base value is nullable.
  /// Throws an error if no type can be extracted.
  private static cidlType(
    type: Type,
    inject: boolean = false,
  ): [CidlType, boolean] {
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
    }, undefined)!;

    return [cidlType, nullable];
  }

  private static includeTree(
    expr: Expression,
    currentClass: ClassDeclaration,
    sf: SourceFile,
  ): CidlIncludeTree {
    if (!expr.isKind || !expr.isKind(SyntaxKind.ObjectLiteralExpression)) {
      return [];
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

      let [cidl_type, _] = CidlExtractor.cidlType(navProp.getType());
      if (typeof cidl_type === "string") continue;

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
          nestedTree = CidlExtractor.includeTree(initializer, targetClass, sf);
        }
      }

      result.push([typedValue, nestedTree]);
    }

    return result;
  }

  private static method(method: MethodDeclaration): ModelMethod {
    const decorators = method.getDecorators();
    const decoratorNames = decorators.map((d) => getDecoratorName(d));

    const httpVerb = decoratorNames.find((name) =>
      Object.values(HttpVerb).includes(name as HttpVerb),
    ) as HttpVerb;

    const parameters: NamedTypedValue[] = [];

    for (const param of method.getParameters()) {
      // Handle injected param
      if (param.getDecorator(ParameterDecoratorKind.Inject)) {
        let [cidl_type, _] = CidlExtractor.cidlType(param.getType(), true);
        parameters.push({
          name: param.getName(),
          cidl_type: cidl_type,
          nullable: false,
        });
        continue;
      }

      // Handle all other params
      let [cidl_type, nullable] = CidlExtractor.cidlType(param.getType());
      parameters.push({
        name: param.getName(),
        cidl_type,
        nullable,
      });
    }

    // TODO: return types cant be nullable??
    let [return_type, _] = CidlExtractor.cidlType(method.getReturnType());

    return {
      name: method.getName(),
      is_static: method.isStatic(),
      http_verb: httpVerb,
      return_type,
      parameters,
    };
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
