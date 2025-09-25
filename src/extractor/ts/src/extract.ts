import path from "node:path";
import {
  Project,
  Type,
  SourceFile,
  PropertyDeclaration,
  MethodDeclaration,
  SyntaxKind,
  ClassDeclaration,
  Decorator,
} from "ts-morph";

const HTTP_VERBS = ["GET", "POST", "PUT", "PATCH", "DELETE"];

// Mirrors the rust bindings
type CidlType =
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | "D1Database"
  | { Model: string }
  | { Array: CidlType }
  | { HttpResult: CidlType | null };

enum AttributeDecoratorKind {
  PrimaryKey = "PrimaryKey",
  ForeignKey = "ForeignKey",
  OneToOne = "OneToOne",
  OneToMany = "OneToMany",
  ManyToMany = "ManyToMany",
  DataSource = "DataSource",
}

export class CidlExtractor {
  constructor(
    public projectName: string,
    public version: string
  ) {}

  extract(project: Project) {
    const models = project.getSourceFiles().flatMap((sourceFile) => {
      return sourceFile
        .getClasses()
        .filter((classDecl) => hasDecorator(classDecl, "D1"))
        .map((classDecl) => CidlExtractor.model(classDecl, sourceFile));
    });

    return {
      version: this.version,
      project_name: this.projectName,
      language: "TypeScript",
      models,
    };
  }

  private static model(classDecl: ClassDeclaration, sourceFile: SourceFile) {
    const className = classDecl.getName() ?? "<anonymous>";
    const attributes: any[] = [];
    const navigationProperties: any[] = [];
    const dataSources: any[] = [];

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
      switch (name) {
        case AttributeDecoratorKind.PrimaryKey: {
          let [cidl_type, nullable] = CidlExtractor.cidlType(prop.getType());
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
          let [cidl_type, nullable] = CidlExtractor.cidlType(prop.getType());
          attributes.push({
            is_primary_key: false,
            foreign_key_reference: getDecoratorArgument(decorator, 0),
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

          let [cidl_type, nullable] = CidlExtractor.cidlType(prop.getType());
          navigationProperties.push({
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
            kind: { [name]: { reference } },
          });
          break;
        }
        case AttributeDecoratorKind.OneToMany:
        case AttributeDecoratorKind.ManyToMany: {
          const reference = getDecoratorArgument(decorator, 0);
          if (!reference) return;

          let [cidl_type, nullable] = CidlExtractor.cidlType(prop.getType());
          navigationProperties.push({
            value: {
              name: prop.getName(),
              cidl_type,
              nullable,
            },
            kind: { [name]: { reference } },
          });
          break;
        }
        case AttributeDecoratorKind.DataSource: {
          const initializer = (prop as any).getInitializer?.();
          const tree = initializer
            ? CidlExtractor.includeTree(initializer, classDecl, sourceFile)
            : [];
          dataSources.push({ name: prop.getName(), tree });
          break;
        }
      }
    }

    const methods = classDecl
      .getMethods()
      .map((m) => CidlExtractor.method(m, sourceFile));

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
  private static cidlType(type: Type): [CidlType, boolean] {
    let map: Record<string, CidlType> = {
      number: "Integer", // TODO: It's wrong to assume number is always an int.
      string: "Text",
      boolean: "Integer",
      Date: "Text",
      D1Database: "D1Database",
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
      if (base === "Result") {
        return { HttpResult: acc == undefined ? null : acc };
      }

      return { Model: base };
    }, undefined)!;

    return [cidlType, nullable];
  }

  private static includeTree(
    obj: any,
    currentClass: ClassDeclaration,
    sf: SourceFile
  ): any[] {
    if (!obj.isKind || !obj.isKind(SyntaxKind.ObjectLiteralExpression)) {
      return [];
    }

    const result: any[] = [];
    for (const prop of obj.getProperties()) {
      if (!prop.isKind(SyntaxKind.PropertyAssignment)) continue;

      let navProp = findPropertyByName(currentClass, prop.getName());
      if (!navProp) {
        console.log(
          `  Warning: Could not find property "${prop.getName()}" in class ${currentClass.getName()}`
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
      let nestedTree: any[] = [];

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

  private static method(method: MethodDeclaration, sf: SourceFile): any {
    const decorators = method.getDecorators();
    const decoratorNames = decorators.map((d) => getDecoratorName(d));

    const httpVerb =
      HTTP_VERBS.find((verb) => decoratorNames.includes(verb)) || null;

    const parameters: any[] = [];

    for (const param of method.getParameters()) {
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
  index: number
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
  name: string
): PropertyDeclaration | undefined {
  // Try exact match first
  const exactMatch = cls.getProperties().find((p) => p.getName() === name);
  return exactMatch;
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
