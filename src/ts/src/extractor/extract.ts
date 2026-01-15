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
  Scope,
  ObjectLiteralExpression,
} from "ts-morph";

import {
  CidlIncludeTree,
  CloesceAst,
  CidlType,
  DataSource,
  HttpVerb,
  Model,
  D1Column,
  ApiMethod,
  NamedTypedValue,
  NavigationProperty,
  WranglerEnv,
  PlainOldObject,
  CrudKind,
  Service,
  defaultMediaType,
  ServiceAttribute,
  KeyValue,
  AstR2Object,
} from "../ast.js";
import { TypeFormatFlags } from "typescript";
import { ExtractorError, ExtractorErrorCode } from "./err.js";
import { HttpResult, KValue } from "../ui/common.js";
import { Either } from "../common.js";

enum PropertyDecoratorKind {
  PrimaryKey = "PrimaryKey",
  ForeignKey = "ForeignKey",
  OneToOne = "OneToOne",
  OneToMany = "OneToMany",
  ManyToMany = "ManyToMany",
  DataSource = "DataSource",
  KeyParam = "KeyParam",
  KV = "KV",
  R2 = "R2",
}

enum ClassDecoratorKind {
  Model = "Model",
  WranglerEnv = "WranglerEnv",
  PlainOldObject = "PlainOldObject",
  Service = "Service",
  CRUD = "CRUD",
}

enum ParameterDecoratorKind {
  Inject = "Inject",
}

export class CidlExtractor {
  static extract(
    projectName: string,
    project: Project,
  ): Either<ExtractorError, CloesceAst> {
    const models: Record<string, Model> = {};
    const poos: Record<string, PlainOldObject> = {};
    const wranglerEnvs: WranglerEnv[] = [];
    const services: Record<string, Service> = {};
    let app_source: string | null = null;

    for (const sourceFile of project.getSourceFiles()) {
      // Check if this is the app source file
      const sourceFiles = ["app.cloesce.ts", "seed__app.cloesce.ts"];
      if (sourceFiles.includes(sourceFile.getBaseName())) {
        const app = CidlExtractor.app(sourceFile);
        if (app.isLeft()) {
          return app;
        }

        app_source = app.unwrap();
      }

      for (const classDecl of sourceFile.getClasses()) {
        const notExportedErr = err(ExtractorErrorCode.MissingExport, (e) => {
          e.context = classDecl.getName();
          e.snippet = classDecl.getText();
        });

        // TODO: put CRUD list inside of the class decorator
        checkDecorators: for (const decorator of classDecl.getDecorators()) {
          const decoratorName = decorator.getName();

          switch (decoratorName) {
            case ClassDecoratorKind.Model: {
              if (!classDecl.isExported()) return notExportedErr;
              const result = ModelExtractor.extract(classDecl, sourceFile);

              if (result.isLeft()) {
                result.value.addContext(
                  (prev) => `${classDecl.getName()}.${prev}`,
                );
                return result;
              }

              const model = result.unwrap();
              models[model.name] = model;
              break checkDecorators;
            }

            case ClassDecoratorKind.Service: {
              if (!classDecl.isExported()) return notExportedErr;
              const result = ServiceExtractor.extract(classDecl, sourceFile);

              if (result.isLeft()) {
                result.value.addContext(
                  (prev) => `${classDecl.getName()}.${prev}`,
                );
                return result;
              }

              const service = result.unwrap();
              services[service.name] = service;
              break checkDecorators;
            }

            case ClassDecoratorKind.PlainOldObject: {
              if (!classDecl.isExported()) return notExportedErr;
              const result = CidlExtractor.poo(classDecl, sourceFile);

              if (result.isLeft()) {
                result.value.addContext(
                  (prev) => `${classDecl.getName()}.${prev}`,
                );
                return result;
              }
              poos[result.unwrap().name] = result.unwrap();
              break checkDecorators;
            }

            case ClassDecoratorKind.WranglerEnv: {
              // Error: invalid property modifier
              for (const prop of classDecl.getProperties()) {
                const modifierRes = checkPropertyModifier(prop);
                if (modifierRes.isLeft()) {
                  return modifierRes;
                }
              }

              const result = CidlExtractor.env(classDecl, sourceFile);
              if (result.isLeft()) {
                return result;
              }

              wranglerEnvs.push(result.unwrap());
              break checkDecorators;
            }

            default: {
              continue;
            }
          }
        }
      }
    }

    // Error: Only one wrangler environment can exist
    if (wranglerEnvs.length > 1) {
      return err(
        ExtractorErrorCode.TooManyWranglerEnvs,
        (e) => (e.context = wranglerEnvs.map((w) => w.name).toString()),
      );
    }

    return Either.right({
      project_name: projectName,
      wrangler_env: wranglerEnvs[0],
      models: models,
      poos,
      services,
      app_source,
    });
  }

  static app(sourceFile: SourceFile): Either<ExtractorError, string> {
    const symbol = sourceFile.getDefaultExportSymbol();
    const decl = symbol?.getDeclarations()[0];

    if (!decl) {
      return err(ExtractorErrorCode.AppMissingDefaultExport);
    }

    const getTypeText = (): string | undefined => {
      let type = undefined;
      if (MorphNode.isExportAssignment(decl)) {
        type = decl.getExpression()?.getType();
      }
      if (MorphNode.isVariableDeclaration(decl)) {
        type = decl.getInitializer()?.getType();
      }
      return type?.getText(
        undefined,
        TypeFormatFlags.UseAliasDefinedOutsideCurrentScope,
      );
    };

    const typeText = getTypeText();
    if (typeText === "CloesceApp") {
      return Either.right(sourceFile.getFilePath().toString());
    }

    return err(ExtractorErrorCode.AppMissingDefaultExport);
  }

  static poo(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, PlainOldObject> {
    const name = classDecl.getName()!;
    const attributes: NamedTypedValue[] = [];

    for (const prop of classDecl.getProperties()) {
      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      // Error: invalid property modifier
      const modifierRes = checkPropertyModifier(prop);
      if (modifierRes.isLeft()) {
        return modifierRes;
      }

      const cidl_type = typeRes.unwrap();
      attributes.push({
        name: prop.getName(),
        cidl_type,
      });
      continue;
    }

    return Either.right({
      name,
      attributes,
      source_path: sourceFile.getFilePath().toString(),
    });
  }

  static env(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, WranglerEnv> {
    const vars: Record<string, CidlType> = {};
    let d1_binding: string | undefined = undefined;
    const kv_bindings: string[] = [];
    const r2_bindings: string[] = [];

    for (const prop of classDecl.getProperties()) {
      // TODO: Support multiple D1 bindings
      if (
        prop
          .getType()
          .getText(
            undefined,
            TypeFormatFlags.UseAliasDefinedOutsideCurrentScope,
          ) === "D1Database"
      ) {
        d1_binding = prop.getName();
        continue;
      }

      if (prop.getType().getSymbol()?.getName() === "KVNamespace") {
        kv_bindings.push(prop.getName());
        continue;
      }

      if (prop.getType().getSymbol()?.getName() === "R2Bucket") {
        r2_bindings.push(prop.getName());
        continue;
      }

      const ty = CidlExtractor.cidlType(prop.getType());
      if (ty.isLeft()) {
        ty.value.context = prop.getName();
        ty.value.snippet = prop.getText();
        return ty;
      }

      vars[prop.getName()] = ty.unwrap();
    }

    return Either.right({
      name: classDecl.getName()!,
      source_path: sourceFile.getFilePath().toString(),
      d1_binding,
      kv_bindings,
      r2_bindings,
      vars,
    });
  }

  private static readonly primTypeMap: Record<string, CidlType> = {
    number: "Real",
    Number: "Real",
    Integer: "Integer",
    string: "Text",
    String: "Text",
    boolean: "Boolean",
    Boolean: "Boolean",
    Date: "DateIso",
    Uint8Array: "Blob",
  };

  static cidlType(
    type: Type,
    inject: boolean = false,
  ): Either<ExtractorError, CidlType> {
    // Void
    if (type.isVoid()) {
      return Either.right("Void");
    }

    // Unknown
    if (type.isUnknown()) {
      return Either.right("JsonValue");
    }

    // Null
    if (type.isNull()) {
      return Either.right({ Nullable: "Void" });
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
      return Either.right(wrapNullable(prim, nullable));
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
      return Either.right(wrapNullable(base, nullable));
    }

    // Single generic
    const genericTy = generics[0];
    const symbolName = unwrappedType.getSymbol()?.getName();
    const aliasName = unwrappedType.getAliasSymbol()?.getName();

    if (aliasName === "DataSourceOf") {
      return Either.right(
        wrapNullable(
          {
            DataSource: genericTy.getText(
              undefined,
              TypeFormatFlags.UseAliasDefinedOutsideCurrentScope,
            ),
          },
          nullable,
        ),
      );
    }

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

      return Either.right(
        wrapNullable(
          {
            Partial: genericTy
              .getText(
                undefined,
                TypeFormatFlags.UseAliasDefinedOutsideCurrentScope,
              )
              .split("|")[0]
              .trim(),
          },
          nullable,
        ),
      );
    }

    if (symbolName === ReadableStream.name) {
      return Either.right(wrapNullable("Stream", nullable));
    }

    if (
      symbolName === Promise.name ||
      aliasName === "IncludeTree" ||
      symbolName === KValue.name
    ) {
      return wrapGeneric(genericTy, nullable, (inner) => inner);
    }

    if (unwrappedType.isArray()) {
      return wrapGeneric(genericTy, nullable, (inner) => ({ Array: inner }));
    }

    if (symbolName === HttpResult.name) {
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
      wrapper: (inner: CidlType) => CidlType,
    ): Either<ExtractorError, CidlType> {
      const res = CidlExtractor.cidlType(t, inject);

      // Error: propogated from `cidlType`
      return res.map((inner) => wrapNullable(wrapper(inner), isNullable));
    }

    function unwrapNullable(ty: Type): [Type, boolean] {
      if (!ty.isUnion()) return [ty, false];

      const unions = ty.getUnionTypes();
      const nonNulls = unions.filter((t) => !t.isNull() && !t.isUndefined());
      const hasNullable = nonNulls.length < unions.length;

      // Booleans seperate into [null, true, false] from the `getUnionTypes` call
      if (
        nonNulls.length === 2 &&
        nonNulls.every((t) => t.isBooleanLiteral())
      ) {
        return [nonNulls[0].getApparentType(), hasNullable];
      }

      return [nonNulls[0] ?? ty, hasNullable];
    }
  }

  static method(
    method: MethodDeclaration,
    verb: HttpVerb,
  ): Either<ExtractorError, ApiMethod> {
    // Error: invalid method scope, must be public
    if (method.getScope() != Scope.Public) {
      return err(ExtractorErrorCode.InvalidApiMethodModifier, (e) => {
        e.context = method.getName();
        e.snippet = method.getText();
      });
    }

    const parameters = [];

    for (const param of method.getParameters()) {
      // Handle injected param
      if (param.getDecorator(ParameterDecoratorKind.Inject)) {
        const typeRes = CidlExtractor.cidlType(param.getType(), true);

        // Error: invalid type
        if (typeRes.isLeft()) {
          typeRes.value.snippet = method.getText();
          typeRes.value.context = param.getName();
          return typeRes;
        }

        parameters.push({
          name: param.getName(),
          cidl_type: typeRes.unwrap(),
        });
        continue;
      }

      // Handle all other params
      const typeRes = CidlExtractor.cidlType(param.getType());

      // Error: invalid type
      if (typeRes.isLeft()) {
        typeRes.value.snippet = method.getText();
        typeRes.value.context = param.getName();
        return typeRes;
      }

      parameters.push({
        name: param.getName(),
        cidl_type: typeRes.unwrap(),
      });
    }

    const typeRes = CidlExtractor.cidlType(method.getReturnType());

    // Error: invalid type
    if (typeRes.isLeft()) {
      typeRes.value.snippet = method.getText();
      return typeRes;
    }

    return Either.right({
      name: method.getName(),
      http_verb: verb,
      is_static: method.isStatic(),
      return_media: defaultMediaType(),
      return_type: typeRes.unwrap(),
      parameters_media: defaultMediaType(),
      parameters,
    });
  }
}

export class ModelExtractor {
  static extract(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, Model> {
    const name = classDecl.getName()!;
    const columns: D1Column[] = [];
    const key_params: string[] = [];
    const kv_objects: KeyValue[] = [];
    const r2_objects: AstR2Object[] = [];
    const navigation_properties: NavigationProperty[] = [];
    const data_sources: Record<string, DataSource> = {};
    const methods: Record<string, ApiMethod> = {};
    const cruds: Set<CrudKind> = new Set<CrudKind>();
    let primary_key: NamedTypedValue | null = null;

    // Extract crud methods
    const crudDecorator = classDecl
      .getDecorators()
      .find((d) => getDecoratorName(d) === ClassDecoratorKind.CRUD);
    if (crudDecorator) {
      setCrudKinds(crudDecorator, cruds);
    }

    // Iterate attribtutes
    for (const prop of classDecl.getProperties()) {
      const decorators = prop.getDecorators();
      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      const checkModifierRes = checkPropertyModifier(prop);

      // No decorators means this is a standard property
      if (decorators.length === 0) {
        // Error: invalid property modifier
        if (checkModifierRes.isLeft()) {
          return checkModifierRes;
        }

        const cidl_type = typeRes.unwrap();
        columns.push({
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
      const decoratorName = getDecoratorName(decorator);

      // Error: invalid property modifier
      if (
        checkModifierRes.isLeft() &&
        decoratorName !== PropertyDecoratorKind.DataSource
      ) {
        return checkModifierRes;
      }

      // Process decorator
      const cidl_type = typeRes.unwrap();
      switch (decoratorName) {
        case PropertyDecoratorKind.PrimaryKey: {
          primary_key = {
            name: prop.getName(),
            cidl_type,
          };
          break;
        }
        case PropertyDecoratorKind.ForeignKey: {
          columns.push({
            foreign_key_reference: getDecoratorArgument(decorator, 0) ?? null,
            value: {
              name: prop.getName(),
              cidl_type,
            },
          });
          break;
        }
        case PropertyDecoratorKind.OneToOne: {
          const reference = getDecoratorArgument(decorator, 0);

          // Error: One to one navigation properties requre a reference
          if (!reference) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              },
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
              },
            );
          }

          navigation_properties.push({
            var_name: prop.getName(),
            model_reference: model_name,
            kind: { OneToOne: { column_reference: reference } },
          });
          break;
        }
        case PropertyDecoratorKind.OneToMany: {
          const reference = getDecoratorArgument(decorator, 0);
          // Error: One to one navigation properties requre a reference
          if (!reference) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              },
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
              },
            );
          }

          navigation_properties.push({
            var_name: prop.getName(),
            model_reference: model_name,
            kind: { OneToMany: { column_reference: reference } },
          });
          break;
        }
        case PropertyDecoratorKind.ManyToMany: {
          // Error: navigation properties require a model reference
          let model_name = getObjectName(cidl_type);
          if (!model_name) {
            return err(
              ExtractorErrorCode.MissingNavigationPropertyReference,
              (e) => {
                e.snippet = prop.getText();
                e.context = prop.getName();
              },
            );
          }

          navigation_properties.push({
            var_name: prop.getName(),
            model_reference: model_name,
            kind: "ManyToMany",
          });
          break;
        }
        case PropertyDecoratorKind.DataSource: {
          const isIncludeTree =
            prop
              .getType()
              .getText(
                undefined,
                TypeFormatFlags.UseAliasDefinedOutsideCurrentScope,
              ) === `IncludeTree<${name}>`;

          // Error: data sources must be static include trees
          if (!prop.isStatic() || !isIncludeTree) {
            return err(ExtractorErrorCode.InvalidDataSourceDefinition, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          const initializer = prop.getInitializer();
          if (!initializer?.isKind(SyntaxKind.ObjectLiteralExpression)) {
            return err(ExtractorErrorCode.InvalidDataSourceDefinition, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          data_sources[prop.getName()] = {
            name: prop.getName(),
            tree: parseIncludeTree(initializer),
          };
          break;
        }
        case PropertyDecoratorKind.KeyParam: {
          key_params.push(prop.getName());
          break;
        }
        case PropertyDecoratorKind.KV: {
          // Format and namespace binding are required
          const format = getDecoratorArgument(decorator, 0);
          const namespace_binding = getDecoratorArgument(decorator, 1);
          if (!format || !namespace_binding) {
            return err(ExtractorErrorCode.InvalidTypescriptSyntax, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          // Ensure that the prop type is KValue<T>
          const ty = prop.getType();
          const isArray = ty.isArray();
          const elementType = isArray ? ty.getArrayElementTypeOrThrow() : ty;
          const symbolName = elementType.getSymbol()?.getName();
          if (symbolName !== KValue.name) {
            return err(ExtractorErrorCode.MissingKValue, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          kv_objects.push({
            format,
            namespace_binding,
            value: {
              name: prop.getName(),
              cidl_type: isArray ? (cidl_type as any).Array : cidl_type,
            },
            list_prefix: isArray,
          });
          break;
        }
        case PropertyDecoratorKind.R2: {
          // Format and bucket binding are required
          const format = getDecoratorArgument(decorator, 0);
          const bucket_binding = getDecoratorArgument(decorator, 1);
          if (!format || !bucket_binding) {
            return err(ExtractorErrorCode.InvalidTypescriptSyntax, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          // Type must be R2ObjectBody
          const ty = prop.getType();
          const isArray = ty.isArray();
          const elementType = isArray ? ty.getArrayElementTypeOrThrow() : ty;
          const symbolName = elementType.getSymbol()?.getName();
          if (symbolName !== "R2ObjectBody") {
            return err(ExtractorErrorCode.MissingR2ObjectBody, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          r2_objects.push({
            format,
            bucket_binding,
            var_name: prop.getName(),
            list_prefix: isArray,
          });
          break;
        }
      }
    }

    // Process methods
    for (const m of classDecl.getMethods()) {
      const httpVerb = m
        .getDecorators()
        .map(getDecoratorName)
        .find((name) => Object.values(HttpVerb).includes(name as HttpVerb)) as
        | HttpVerb
        | undefined;

      if (!httpVerb) {
        continue;
      }

      const result = CidlExtractor.method(m, httpVerb);
      if (result.isLeft()) {
        result.value.addContext((prev) => `${m.getName()} ${prev}`);
        return result;
      }
      methods[result.unwrap().name] = result.unwrap();
    }

    return Either.right({
      name,
      columns,
      primary_key,
      navigation_properties,
      key_params,
      kv_objects,
      r2_objects,
      methods,
      data_sources,
      cruds: Array.from(cruds).sort(),
      source_path: sourceFile.getFilePath().toString(),
    });
  }
}

export class ServiceExtractor {
  static extract(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, Service> {
    const attributes: ServiceAttribute[] = [];
    const methods: Record<string, ApiMethod> = {};

    // Properties
    for (const prop of classDecl.getProperties()) {
      const typeRes = CidlExtractor.cidlType(prop.getType(), true);

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      if (typeof typeRes.value === "string" || !("Inject" in typeRes.value)) {
        return err(ExtractorErrorCode.InvalidServiceProperty, (e) => {
          e.context = prop.getName();
          e.snippet = prop.getText();
        });
      }

      // Error: invalid property modifier
      const checkModifierRes = checkPropertyModifier(prop);
      if (checkModifierRes.isLeft()) {
        return checkModifierRes;
      }

      attributes.push({
        var_name: prop.getName(),
        inject_reference: typeRes.value.Inject,
      });
    }

    // Methods
    for (const m of classDecl.getMethods()) {
      const httpVerb = m
        .getDecorators()
        .map(getDecoratorName)
        .find((name) =>
          Object.values(HttpVerb).includes(name as HttpVerb),
        ) as HttpVerb;

      if (!httpVerb) {
        continue;
      }

      const res = CidlExtractor.method(m, httpVerb);
      if (res.isLeft()) {
        return res;
      }

      const serviceMethod = res.unwrap();
      methods[serviceMethod.name] = serviceMethod;
    }

    return Either.right({
      name: classDecl.getName()!,
      attributes,
      methods,
      source_path: sourceFile.getFilePath().toString(),
    });
  }
}

function err(
  code: ExtractorErrorCode,
  fn?: (extractorErr: ExtractorError) => void,
): Either<ExtractorError, never> {
  let e = new ExtractorError(code);
  if (fn) {
    fn(e);
  }
  return Either.left(e);
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

  if (arg.getKind?.() === SyntaxKind.Identifier) {
    return arg.getText();
  }

  return arg.getLiteralValue();
}

function getRootType(t: CidlType): CidlType {
  if (typeof t === "string") {
    return t;
  }

  if ("Nullable" in t) {
    return getRootType(t.Nullable);
  }

  if ("Array" in t) {
    return getRootType(t.Array);
  }

  if ("HttpResult" in t) {
    return getRootType(t.HttpResult);
  }

  return t;
}

function getObjectName(t: CidlType): string | undefined {
  const root = getRootType(t);
  if (typeof root !== "string" && "Object" in root) {
    return root["Object"];
  }

  return undefined;
}

function setCrudKinds(d: Decorator, cruds: Set<CrudKind>) {
  const arg = d.getArguments()[0];
  if (!arg) {
    return;
  }

  if (MorphNode.isArrayLiteralExpression(arg)) {
    for (const a of arg.getElements()) {
      cruds.add(
        (MorphNode.isStringLiteral(a)
          ? a.getLiteralValue()
          : a.getText()) as CrudKind,
      );
    }
  }
}

function parseIncludeTree(
  objLiteral: ObjectLiteralExpression,
): CidlIncludeTree {
  const result: CidlIncludeTree = {};

  objLiteral.getProperties().forEach((prop) => {
    if (prop.isKind(SyntaxKind.PropertyAssignment)) {
      const name = prop.getName();
      const init = prop.getInitializer();

      // Check if it's a nested object literal
      if (init?.isKind(SyntaxKind.ObjectLiteralExpression)) {
        result[name] = parseIncludeTree(init); // Recurse
      } else {
        result[name] = {}; // Empty object by default
      }
    }
  });

  return result;
}

function checkPropertyModifier(
  prop: PropertyDeclaration,
): Either<ExtractorError, null> {
  // Error: properties must be just 'public'
  if (prop.getScope() != Scope.Public || prop.isReadonly() || prop.isStatic()) {
    return err(ExtractorErrorCode.InvalidPropertyModifier, (e) => {
      e.context = prop.getName();
      e.snippet = prop.getText();
    });
  }
  return Either.right(null);
}
