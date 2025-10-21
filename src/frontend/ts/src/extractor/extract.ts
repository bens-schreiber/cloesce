import {
  Node as MorphNode,
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
  PlainOldObject,
  CrudKind,
} from "../common.js";
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
  PlainOldObject = "PlainOldObject",
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
    const poos: Record<string, PlainOldObject> = {};
    const wranglerEnvs: WranglerEnv[] = [];
    let app_source: string | null = null;

    for (const sourceFile of project.getSourceFiles()) {
      if (
        sourceFile.getBaseName() === "app.cloesce.ts" ||
        sourceFile.getBaseName() === "seed__app.cloesce.ts" // hardcoding for tests
      ) {
        const app = CidlExtractor.app(sourceFile);
        if (!app.ok) {
          return app;
        }

        app_source = app.value;
      }

      for (const classDecl of sourceFile.getClasses()) {
        if (hasDecorator(classDecl, ClassDecoratorKind.D1)) {
          const result = CidlExtractor.model(classDecl, sourceFile);

          // Error: propogate from models
          if (!result.ok) {
            result.value.addContext((old) => `${classDecl.getName()}.${old}`);
            return result;
          }
          models[result.value.name] = result.value;
          continue;
        }

        if (hasDecorator(classDecl, ClassDecoratorKind.PlainOldObject)) {
          const result = CidlExtractor.poo(classDecl, sourceFile);

          // Error: propogate from models
          if (!result.ok) {
            result.value.addContext((old) => `${classDecl.getName()}.${old}`);
            return result;
          }
          poos[result.value.name] = result.value;
          continue;
        }

        if (hasDecorator(classDecl, ClassDecoratorKind.WranglerEnv)) {
          wranglerEnvs.push({
            name: classDecl.getName(),
            source_path: sourceFile.getFilePath().toString(),
          } as WranglerEnv);
        }
      }
    }

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
      poos,
      app_source,
    });
  }

  private static app(sourceFile: SourceFile): Either<ExtractorError, string> {
    const symbol = sourceFile.getDefaultExportSymbol();
    const decl = symbol?.getDeclarations()[0];

    if (!decl) {
      return err(ExtractorErrorCode.AppMissingDefaultExport);
    }

    const getTypeText = (): string | undefined => {
      if (MorphNode.isExportAssignment(decl)) {
        return decl.getExpression()?.getType().getText();
      }
      if (MorphNode.isVariableDeclaration(decl)) {
        return decl.getInitializer()?.getType().getText();
      }
      return undefined;
    };

    const typeText = getTypeText();
    if (typeText === "CloesceApp") {
      return right(sourceFile.getFilePath().toString());
    }

    return err(ExtractorErrorCode.AppMissingDefaultExport);
  }

  private static model(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile
  ): Either<ExtractorError, Model> {
    const name = classDecl.getName()!;
    const attributes: ModelAttribute[] = [];
    const navigationProperties: NavigationProperty[] = [];
    const dataSources: Record<string, DataSource> = {};
    const methods: Record<string, ModelMethod> = {};
    let cruds: CrudKind[] = [];
    let primary_key: NamedTypedValue | undefined = undefined;

    // Extract crud methods
    const crudDecorator = classDecl
      .getDecorators()
      .find((d) => getDecoratorName(d) === "CRUD");
    if (crudDecorator) {
      cruds = getCrudKinds(crudDecorator);
    }

    // Iterate attribtutes
    for (const prop of classDecl.getProperties()) {
      const decorators = prop.getDecorators();
      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (!typeRes.ok) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
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

          let model_name = getObjectName(cidl_type);

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

          let model_name = getObjectName(cidl_type);

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
          let model_name = getObjectName(cidl_type);
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
            treeRes.value.snippet = prop.getText();
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
      return err(ExtractorErrorCode.MissingPrimaryKey, (e) => {
        e.snippet = classDecl.getText();
      });
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
      cruds,
      data_sources: dataSources,
      source_path: sourceFile.getFilePath().toString(),
    });
  }

  private static poo(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile
  ): Either<ExtractorError, PlainOldObject> {
    const name = classDecl.getName()!;
    const attributes: NamedTypedValue[] = [];

    for (const prop of classDecl.getProperties()) {
      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (!typeRes.ok) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      const cidl_type = typeRes.value;
      attributes.push({
        name: prop.getName(),
        cidl_type,
      });
      continue;
    }

    return right({
      name,
      attributes,
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

    // No generics -> inject or object
    if (generics.length === 0) {
      const base = inject ? { Inject: tyText } : { Object: tyText };
      return right(wrapNullable(base, nullable));
    }

    // Single generic
    const genericTy = generics[0];
    const symbolName = unwrappedType.getSymbol()?.getName();
    const aliasName = unwrappedType.getAliasSymbol()?.getName();

    if (aliasName === "DeepPartial") {
      const [_, genericTyNullable] = unwrapNullable(genericTy);
      const genericTyGenerics = [
        ...genericTy.getAliasTypeArguments(),
        ...genericTy.getTypeArguments(),
      ];

      // Expect partials to be of the exact form DeepPartial<Model>
      if (
        genericTyNullable ||
        genericTy.isUnion() ||
        genericTyGenerics.length > 0
      ) {
        return err(ExtractorErrorCode.InvalidPartialType);
      }

      return right(
        wrapNullable(
          {
            Partial: genericTy
              .getText(
                undefined,
                TypeFormatFlags.UseAliasDefinedOutsideCurrentScope
              )
              .split("|")[0]
              .trim(),
          },
          nullable
        )
      );
    }

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
      return err(ExtractorErrorCode.InvalidIncludeTree);
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
        const targetModel = getObjectName(cidl_type);
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

  if (arg.getKind?.() === SyntaxKind.Identifier) {
    return arg.getText();
  }

  return arg.getLiteralValue();
}

function getObjectName(t: CidlType): string | undefined {
  if (typeof t === "string") return undefined;

  if ("Object" in t) {
    return t.Object;
  } else if ("Array" in t) {
    return getObjectName(t.Array);
  } else if ("HttpResult" in t) {
    if (t == null) return undefined;
    return getObjectName(t.HttpResult!);
  }

  return undefined;
}

function getCrudKinds(d: Decorator): CrudKind[] {
  const arg = d.getArguments()[0];
  if (!arg) return [];

  if (MorphNode.isArrayLiteralExpression(arg)) {
    return arg
      .getElements()
      .map(
        (e) =>
          (MorphNode.isStringLiteral(e)
            ? e.getLiteralValue()
            : e.getText()) as CrudKind
      );
  }

  return [];
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
