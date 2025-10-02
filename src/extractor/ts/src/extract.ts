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
  CloesceAst,
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
import { TypeFormatFlags } from "typescript";

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

  extract(project: Project): Either<string, CloesceAst> {
    const models: Record<string, Model> = {};
    for (const sourceFile of project.getSourceFiles()) {
      for (const classDecl of sourceFile.getClasses()) {
        if (!hasDecorator(classDecl, ClassDecoratorKind.D1)) continue;

        const result = CidlExtractor.model(classDecl, sourceFile);
        if (!result.ok) {
          return left(result.value);
        }
        models[result.value.name] = result.value;
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
      return left("Missing wrangler environment @WranglerEnv");
    }
    if (wranglerEnvs.length > 1) {
      return left("Too many wrangler environments specified with @WranglerEnv");
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
    const dataSources: Record<string, DataSource> = {};
    const methods: Record<string, ModelMethod> = {};
    let primary_key: NamedTypedValue | undefined = undefined;

    for (const prop of classDecl.getProperties()) {
      const decorators = prop.getDecorators();

      // No decorators means this is a standard attribute
      if (decorators.length === 0) {
        const typeRes = CidlExtractor.cidlType(prop.getType());
        if (!typeRes.ok) {
          return typeRes;
        }

        const cidl_type = typeRes.value;
        attributes.push({
          foreign_key_reference: null,
          value: {
            name: prop.getName(),
            cidl_type,
          },
        });
        continue;
      }

      // TODO: Limiting to one decorator. Can't get too fancy on us.
      const decorator = decorators[0];
      const name = getDecoratorName(decorator);

      const typeRes = CidlExtractor.cidlType(prop.getType());
      if (!typeRes.ok) {
        return typeRes;
      }

      // Process decorators
      const cidl_type = typeRes.value;
      switch (name) {
        case AttributeDecoratorKind.PrimaryKey: {
          primary_key = {
            name: prop.getName(),
            cidl_type,
          };
          break;
        }
        case AttributeDecoratorKind.ForeignKey: {
          attributes.push({
            foreign_key_reference: getDecoratorArgument(decorator, 0) ?? null,
            value: {
              name: prop.getName(),
              cidl_type,
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

          let model_name = getModelName(cidl_type);
          if (!model_name) {
            return left(
              `One to One Navigation properties must hold model types ${name}.${prop.getName()}`,
            );
          }

          navigationProperties.push({
            var_name: prop.getName(),
            model_name,
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

          let model_name = getModelName(cidl_type);
          if (!model_name) {
            return left(
              `One to Many Navigation properties must hold model types ${name}.${prop.getName()}`,
            );
          }

          navigationProperties.push({
            var_name: prop.getName(),
            model_name,
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

          let model_name = getModelName(cidl_type);
          if (!model_name) {
            return left(
              `Many to Many Navigation properties must hold model types ${name}.${prop.getName()}`,
            );
          }
          navigationProperties.push({
            var_name: prop.getName(),
            model_name,
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

          const treeRes = CidlExtractor.includeTree(
            initializer,
            classDecl,
            sourceFile,
          );
          if (!treeRes.ok) {
            return treeRes;
          }

          dataSources[prop.getName()] = {
            name: prop.getName(),
            tree: treeRes.value,
          };
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
      methods[result.value.name] = result.value;
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

  private static readonly primTypeMap: Record<string, CidlType> = {
    number: "Integer",
    Number: "Integer",
    string: "Text",
    String: "Text",
    boolean: "Integer",
    Boolean: "Integer",
    Date: "Text",
  };

  private static cidlType(
    type: Type,
    inject: boolean = false,
  ): Either<string, CidlType> {
    // Void
    if (type.isVoid()) {
      return right("Void");
    }

    // Null
    if (type.isNull()) {
      return right({ Nullable: "Void" });
    }

    // Nullable via union
    const [unwrappedType, nullable] = unwrapNullable(type);

    const tyText = unwrappedType
      .getText(undefined, TypeFormatFlags.UseAliasDefinedOutsideCurrentScope)
      .split("|")[0]
      .trim();

    // Primitives
    const prim = this.primTypeMap[tyText];
    if (prim) {
      return right(wrapNullable(prim, nullable));
    }

    const generics = [
      ...unwrappedType.getAliasTypeArguments(),
      ...unwrappedType.getTypeArguments(),
    ];

    if (generics.length > 1) {
      return left("Multiple generics are not yet supported");
    }

    // No generics -> inject or model
    if (generics.length === 0) {
      const base = inject ? { Inject: tyText } : { Model: tyText };
      return right(wrapNullable(base, nullable));
    }

    // Single generic
    const genericTy = generics[0];
    const symbolName = unwrappedType.getSymbol()?.getName();
    const aliasName = unwrappedType.getAliasSymbol()?.getName();

    if (symbolName === "Promise" || aliasName === "IncludeTree") {
      return wrapGeneric(genericTy, nullable, (inner) => inner);
    }

    if (unwrappedType.isArray()) {
      return wrapGeneric(genericTy, nullable, (inner) => ({ Array: inner }));
    }

    if (aliasName === "HttpResult") {
      return wrapGeneric(genericTy, nullable, (inner) => ({
        HttpResult: inner,
      }));
    }

    return left(`Unknown symbol ${tyText}`);

    function wrapNullable(inner: CidlType, isNullable: boolean): CidlType {
      if (isNullable) {
        return { Nullable: inner };
      } else {
        return inner;
      }
    }

    function wrapGeneric(
      t: Type,
      isNullable: boolean,
      wrapper: (inner: CidlType) => CidlType,
    ): Either<string, CidlType> {
      const res = CidlExtractor.cidlType(t, inject);
      if (!res.ok) {
        return res;
      }
      return right(wrapNullable(wrapper(res.value), isNullable));
    }

    function unwrapNullable(ty: Type): [Type, boolean] {
      if (ty.isUnion()) {
        const nonNull = ty.getUnionTypes().filter((t) => !t.isNull());
        if (nonNull.length === 1) {
          return [nonNull[0], true];
        }
      }
      return [ty, false];
    }
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

    const result: CidlIncludeTree = {};
    for (const prop of expr.getProperties()) {
      if (!prop.isKind(SyntaxKind.PropertyAssignment)) continue;

      const navProp = findPropertyByName(currentClass, prop.getName());
      if (!navProp) {
        console.log(
          `  Warning: Could not find property "${prop.getName()}" in class ${currentClass.getName()}`,
        );
        continue;
      }

      const typeRes = CidlExtractor.cidlType(navProp.getType());
      if (!typeRes.ok) {
        return typeRes;
      }

      const cidl_type = typeRes.value;
      if (typeof cidl_type === "string") {
        return left(`Invalid include tree type ${cidl_type} expected Model`);
      }

      // Recurse for nested includes
      const initializer = (prop as any).getInitializer?.();
      let nestedTree: CidlIncludeTree = {};
      if (initializer?.isKind?.(SyntaxKind.ObjectLiteralExpression)) {
        const targetModel = getModelName(cidl_type);
        const targetClass = currentClass
          .getSourceFile()
          .getProject()
          .getSourceFiles()
          .flatMap((f) => f.getClasses())
          .find((c) => c.getName() === targetModel);

        if (targetClass) {
          const treeRes = CidlExtractor.includeTree(
            initializer,
            targetClass,
            sf,
          );
          if (!treeRes.ok) {
            return treeRes;
          }
          nestedTree = treeRes.value;
        }
      }

      result[navProp.getName()] = nestedTree;
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
        const typeRes = CidlExtractor.cidlType(param.getType(), true);
        if (!typeRes.ok) {
          return typeRes;
        }

        parameters.push({
          name: param.getName(),
          cidl_type: typeRes.value,
        });
        continue;
      }

      // Handle all other params
      const typeRes = CidlExtractor.cidlType(param.getType());
      if (!typeRes.ok) {
        return typeRes;
      }

      parameters.push({
        name: param.getName(),
        cidl_type: typeRes.value,
      });
    }

    // TODO: return types cant be nullable??
    const typeRes = CidlExtractor.cidlType(method.getReturnType());
    if (!typeRes.ok) {
      return typeRes;
    }

    return right({
      name: method.getName(),
      is_static: method.isStatic(),
      http_verb: httpVerb,
      return_type: typeRes.value,
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
