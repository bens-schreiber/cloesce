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
  ExtractorError,
  ExtractorErrorCode,
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
    public version: string
  ) {}

  extract(project: Project): Either<ExtractorError, CloesceAst> {
    const models: Record<string, Model> = {};
    for (const sourceFile of project.getSourceFiles()) {
      for (const classDecl of sourceFile.getClasses()) {
        if (!hasDecorator(classDecl, ClassDecoratorKind.D1)) continue;

        const result = CidlExtractor.model(classDecl, sourceFile);

        // Error: propogate from models
        if (!result.ok) {
          result.value.addContext((old) => `${classDecl.getName()}.${old}`);
          return result;
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
            hasDecorator(classDecl, ClassDecoratorKind.WranglerEnv)
          )
          .map((classDecl) => {
            return {
              name: classDecl.getName(),
              source_path: sourceFile.getFilePath().toString(),
            } as WranglerEnv;
          });
      });

    // Error: A wrangler environment is required
    if (wranglerEnvs.length < 1) {
      return err(ExtractorErrorCode.MissingWranglerEnv);
    }

    // Error: Only one wrangler environment can exist
    if (wranglerEnvs.length > 1) {
      return err(
        ExtractorErrorCode.TooManyWranglerEnvs,
        (e) => (e.context = wranglerEnvs.map((w) => w.name).toString())
      );
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
    sourceFile: SourceFile
  ): Either<ExtractorError, Model> {
    const name = classDecl.getName() ?? "<anonymous>";
    const attributes: ModelAttribute[] = [];
    const navigationProperties: NavigationProperty[] = [];
    const dataSources: Record<string, DataSource> = {};
    const methods: Record<string, ModelMethod> = {};
    let primary_key: NamedTypedValue | undefined = undefined;

    for (const prop of classDecl.getProperties()) {
      const decorators = prop.getDecorators();
      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (!typeRes.ok) {
        typeRes.value.context = prop.getName();
        return typeRes;
      }

      // No decorators means this is a standard attribute
      if (decorators.length === 0) {
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

          // Error: One to one navigation properties requre a reference
          if (!reference) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              }
            );
          }

          let model_name = getModelName(cidl_type);

          // Error: navigation properties require a model reference
          if (!model_name) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              }
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
          // Error: One to one navigation properties requre a reference
          if (!reference) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              }
            );
          }

          let model_name = getModelName(cidl_type);

          // Error: navigation properties require a model reference
          if (!model_name) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              }
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

          // Error: many to many attribtues require a unique id
          if (!unique_id)
            return err(ExtractorErrorCode.MissingManyToManyUniqueId, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });

          // Error: navigation properties require a model reference
          let model_name = getModelName(cidl_type);
          if (!model_name) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              }
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
          const treeRes = CidlExtractor.includeTree(
            initializer,
            classDecl,
            sourceFile
          );

          if (!treeRes.ok) {
            treeRes.value.addContext((old) => `${prop.getName()} ${old}`);
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
      return err(ExtractorErrorCode.MissingPrimaryKey);
    }

    // Process methods
    for (const m of classDecl.getMethods()) {
      const result = CidlExtractor.method(m);
      if (!result.ok) {
        result.value.addContext((old) => `${m.getName()} ${old}`);
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
    inject: boolean = false
  ): Either<ExtractorError, CidlType> {
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

    // Error: can't handle multiple generics
    if (generics.length > 1) {
      return err(ExtractorErrorCode.MultipleGenericType);
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

    // Error: unknown type
    return err(ExtractorErrorCode.UnknownType);

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
      wrapper: (inner: CidlType) => CidlType
    ): Either<ExtractorError, CidlType> {
      const res = CidlExtractor.cidlType(t, inject);

      // Error: propogated from `cidlType`
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

  private static includeTree(
    expr: Expression | undefined,
    currentClass: ClassDeclaration,
    sf: SourceFile
  ): Either<ExtractorError, CidlIncludeTree> {
    // Include trees must be of the expected form
    if (
      !expr ||
      !expr.isKind ||
      !expr.isKind(SyntaxKind.ObjectLiteralExpression)
    ) {
      return err(
        ExtractorErrorCode.InvalidIncludeTree,
        expr ? (e) => (e.snippet = expr.getText()) : undefined
      );
    }

    const result: CidlIncludeTree = {};
    for (const prop of expr.getProperties()) {
      if (!prop.isKind(SyntaxKind.PropertyAssignment)) continue;

      // Error: navigation property not found
      const navProp = findPropertyByName(currentClass, prop.getName());
      if (!navProp) {
        return err(
          ExtractorErrorCode.UnknownNavigationPropertyReference,
          (e) => {
            e.snippet = expr.getText();
            e.context = prop.getName();
          }
        );
      }

      const typeRes = CidlExtractor.cidlType(navProp.getType());

      // Error: invalid referenced nav prop type
      if (!typeRes.ok) {
        typeRes.value.snippet = navProp.getText();
        typeRes.value.context = prop.getName();
        return typeRes;
      }

      // Error: invalid referenced nav prop type
      const cidl_type = typeRes.value;
      if (typeof cidl_type === "string") {
        return err(
          ExtractorErrorCode.InvalidNavigationPropertyReference,
          (e) => {
            ((e.snippet = navProp.getText()), (e.context = prop.getName()));
          }
        );
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
            sf
          );

          // Error: Propogated from `includeTree`
          if (!treeRes.ok) {
            treeRes.value.snippet = expr.getText();
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
    method: MethodDeclaration
  ): Either<ExtractorError, ModelMethod> {
    const decorators = method.getDecorators();
    const decoratorNames = decorators.map((d) => getDecoratorName(d));

    const httpVerb = decoratorNames.find((name) =>
      Object.values(HttpVerb).includes(name as HttpVerb)
    ) as HttpVerb;

    const parameters: NamedTypedValue[] = [];

    for (const param of method.getParameters()) {
      // Handle injected param
      if (param.getDecorator(ParameterDecoratorKind.Inject)) {
        const typeRes = CidlExtractor.cidlType(param.getType(), true);

        // Error: invalid type
        if (!typeRes.ok) {
          typeRes.value.snippet = method.getText();
          typeRes.value.context = param.getName();
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

      // Error: invalid type
      if (!typeRes.ok) {
        typeRes.value.snippet = method.getText();
        typeRes.value.context = param.getName();
        return typeRes;
      }

      parameters.push({
        name: param.getName(),
        cidl_type: typeRes.value,
      });
    }

    const typeRes = CidlExtractor.cidlType(method.getReturnType());

    // Error: invalid type
    if (!typeRes.ok) {
      typeRes.value.snippet = method.getText();
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

function err(
  code: ExtractorErrorCode,
  fn?: (extractorErr: ExtractorError) => void
): Either<ExtractorError, never> {
  let e = new ExtractorError(code);
  if (fn) {
    fn(e);
  }
  return left(e);
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
