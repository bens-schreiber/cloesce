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
import { Either } from "../common.js";

enum PropertyDecoratorKind {
  PrimaryKey = "PrimaryKey",
  ForeignKey = "ForeignKey",
  OneToOne = "OneToOne",
  OneToMany = "OneToMany",
  ManyToMany = "ManyToMany",
  KeyParam = "KeyParam",
  KV = "KV",
  R2 = "R2",
}

enum ClassDecoratorKind {
  Model = "Model",
  WranglerEnv = "WranglerEnv",
  Service = "Service",
}

enum ParameterDecoratorKind {
  Inject = "Inject",
}

export class CidlExtractor {
  private constructor(
    private modelDecls: Map<string, [ClassDeclaration, Decorator]>,
    private extractedPoos: Map<string, PlainOldObject> = new Map(),
  ) {}

  static extract(
    projectName: string,
    project: Project,
  ): Either<ExtractorError, CloesceAst> {
    const modelDecls: Map<string, [ClassDeclaration, Decorator]> = new Map();
    const serviceDecls: Map<string, ClassDeclaration> = new Map();
    const wranglerEnvs: WranglerEnv[] = [];
    let main_source: string | null = null;

    // TODO: Concurrently across several threads?
    for (const sourceFile of project.getSourceFiles()) {
      // Extract main source
      const mainRes = CidlExtractor.main(sourceFile);
      if (mainRes.isLeft()) {
        return mainRes;
      }
      const main = mainRes.unwrap();
      if (main) {
        main_source = main;
      }

      for (const classDecl of sourceFile.getClasses()) {
        const notExportedErr = err(ExtractorErrorCode.MissingExport, (e) => {
          e.context = classDecl.getName();
          e.snippet = classDecl.getText();
        });

        for (const decorator of classDecl.getDecorators()) {
          const decoratorName = decorator.getName();

          switch (decoratorName) {
            case ClassDecoratorKind.Model: {
              if (!classDecl.isExported()) return notExportedErr;
              modelDecls.set(classDecl.getName()!, [classDecl, decorator]);
              break;
            }

            case ClassDecoratorKind.Service: {
              if (!classDecl.isExported()) return notExportedErr;
              serviceDecls.set(classDecl.getName()!, classDecl);
              break;
            }

            case ClassDecoratorKind.WranglerEnv: {
              const res = CidlExtractor.env(classDecl, sourceFile);
              if (res.isLeft()) {
                res.value.addContext(
                  (prev) => `${classDecl.getName()}.${prev}`,
                );
                return res;
              }

              const wranglerEnv = res.unwrap();
              wranglerEnvs.push(wranglerEnv);
              break;
            }

            default: {
              continue;
            }
          }
        }
      }
    }

    const extractor = new CidlExtractor(modelDecls);

    // Extract models
    const models: Record<string, Model> = {};
    for (const [_, [classDecl, decorator]] of modelDecls) {
      const res = extractor.model(
        classDecl,
        classDecl.getSourceFile(),
        decorator,
      );
      if (res.isLeft()) {
        res.value.addContext((prev) => `${classDecl.getName()}.${prev}`);
        return res;
      }

      const model = res.unwrap();
      models[model.name] = model;
    }

    // Extract services
    const services: Record<string, Service> = {};
    for (const [_, classDecl] of serviceDecls) {
      const res = extractor.service(classDecl, classDecl.getSourceFile());
      if (res.isLeft()) {
        res.value.addContext((prev) => `${classDecl.getName()}.${prev}`);
        return res;
      }

      const service = res.unwrap();
      services[service.name] = service;
    }

    // Error: Only one wrangler environment can exist
    if (wranglerEnvs.length > 1) {
      return err(
        ExtractorErrorCode.TooManyWranglerEnvs,
        (e) => (e.context = wranglerEnvs.map((w) => w.name).toString()),
      );
    }

    const poos = Object.fromEntries(extractor.extractedPoos);
    return Either.right({
      project_name: projectName,
      wrangler_env: wranglerEnvs[0], // undefined if none
      models,
      poos,
      services,
      main_source,
    });
  }

  /**
   * @returns An error if the main function is invalid, or the source code of the app function if valid.
   * Undefined if no main function is defined.
   */
  private static main(
    sourceFile: SourceFile,
  ): Either<ExtractorError, string | undefined> {
    const symbol = sourceFile.getDefaultExportSymbol();
    const decl = symbol?.getDeclarations()[0];

    if (!decl || !MorphNode.isFunctionDeclaration(decl)) {
      return Either.right(undefined);
    }

    // Must be named "main"
    const name = decl.getName();
    if (name !== "main") {
      return Either.right(undefined);
    }

    // Must be async
    if (!decl.isAsync()) {
      return err(
        ExtractorErrorCode.InvalidMain,
        (e) => (e.context = "Missing async modifier"),
      );
    }

    // Must have exactly 4 parameters
    const params = decl.getParameters();
    if (params.length !== 4) {
      return err(ExtractorErrorCode.InvalidMain, (e) => {
        e.context = `Expected 4 parameters, got ${params.length}`;
      });
    }

    // Expected parameter types in order
    // WranglerEnv does not have a required type annotation
    const expectedTypes = ["Request", null, "CloesceApp", "ExecutionContext"];

    for (let i = 0; i < params.length; i++) {
      const param = params[i];
      const expectedType = expectedTypes[i];

      if (expectedType === null) {
        continue;
      }

      const paramType = param.getType();

      const symbol =
        paramType.getAliasSymbol() ??
        paramType.getSymbol() ??
        paramType.getTargetType()?.getSymbol();

      if (symbol?.getName() !== expectedType) {
        return err(ExtractorErrorCode.InvalidMain, (e) => {
          e.context = `Expected parameter ${i + 1} to be of type ${expectedType}, got ${paramType}`;
        });
      }
    }

    // Must return Response
    const returnType = getTypeText(decl.getReturnType());

    if (returnType !== "Promise<Response>") {
      return err(ExtractorErrorCode.InvalidMain, (e) => {
        e.context = `Expected return type to be Promise<Response>, got ${returnType}`;
      });
    }

    return Either.right(sourceFile.getFilePath().toString());
  }

  private model(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
    decorator: Decorator,
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
    const arg = decorator.getArguments()[0];
    if (arg && MorphNode.isArrayLiteralExpression(arg)) {
      for (const a of arg.getElements()) {
        cruds.add(
          (MorphNode.isStringLiteral(a)
            ? a.getLiteralValue()
            : a.getText()) as CrudKind,
        );
      }
    }

    // Iterate properties
    for (const prop of classDecl.getProperties()) {
      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      const cidl_type = typeRes.unwrap();

      // Include Trees
      const isIncludeTree =
        getTypeText(prop.getType()) === `IncludeTree<${name}>`;
      if (isIncludeTree) {
        // Error: data sources must be static include trees
        if (!prop.isStatic()) {
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
        continue;
      }

      const checkModifierRes = checkPropertyModifier(prop);
      // Error: invalid property modifier
      if (checkModifierRes.isLeft()) {
        return checkModifierRes;
      }

      // Infer decorator
      if (prop.getDecorators().length === 0) {
        this.inferModelDecorator(prop, classDecl, cidl_type);
      }
      const decorators = prop.getDecorators();

      // Scalar column
      if (decorators.length === 0) {
        columns.push({
          foreign_key_reference: null,
          value: {
            name: prop.getName(),
            cidl_type,
          },
        });
        continue;
      }

      const decorator = decorators[0];
      const decoratorName = getDecoratorName(decorator);
      const model_reference = getObjectName(cidl_type);

      // Process decorator
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
          const selector = getSelectorPropertyName(decorator);
          if (selector.isLeft()) {
            return err(ExtractorErrorCode.InvalidSelectorSyntax, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          // Error: navigation properties require a model reference
          if (!model_reference) {
            return err(ExtractorErrorCode.InvalidSelectorSyntax, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          navigation_properties.push({
            var_name: prop.getName(),
            model_reference,
            kind: { OneToOne: { column_reference: selector.unwrap() } },
          });
          break;
        }
        case PropertyDecoratorKind.OneToMany: {
          const selector = getSelectorPropertyName(decorator);
          if (selector.isLeft()) {
            return err(ExtractorErrorCode.InvalidSelectorSyntax, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          // Error: navigation properties require a model reference
          if (!model_reference) {
            return err(ExtractorErrorCode.InvalidNavigationProperty, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          navigation_properties.push({
            var_name: prop.getName(),
            model_reference,
            kind: { OneToMany: { column_reference: selector.unwrap() } },
          });
          break;
        }
        case PropertyDecoratorKind.ManyToMany: {
          // Error: navigation properties require a model reference
          if (!model_reference) {
            return err(ExtractorErrorCode.InvalidNavigationProperty, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          navigation_properties.push({
            var_name: prop.getName(),
            model_reference,
            kind: "ManyToMany",
          });
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

          // Ensure that the prop type is a KvObject
          const isArray = typeof cidl_type === "object" && "Array" in cidl_type;
          const unwrapped = isArray ? (cidl_type as any).Array : cidl_type;
          if (!("KvObject" in unwrapped)) {
            return err(ExtractorErrorCode.MissingKValue, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }
          const inner = unwrapped.KvObject;

          if (typeof inner === "object" && "Object" in inner) {
            if (
              !this.extractedPoos.has(inner.Object) &&
              !this.modelDecls.has(inner.Object)
            ) {
              const res = this.poo(
                classDecl.getSourceFile().getClassOrThrow(inner.Object),
                classDecl.getSourceFile(),
              );

              if (res.isLeft()) {
                res.value.addContext((prev) => `${prop.getName()}.${prev}`);
                return res;
              }
            }
          }

          kv_objects.push({
            format,
            namespace_binding,
            value: {
              name: prop.getName(),
              cidl_type: inner,
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

          // Type must be R2Object
          const isArray = typeof cidl_type === "object" && "Array" in cidl_type;
          const unwrapped = isArray ? (cidl_type as any).Array : cidl_type;
          if (unwrapped !== "R2Object") {
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

      const result = this.method(m, httpVerb);
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

  private service(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, Service> {
    const attributes: ServiceAttribute[] = [];
    const methods: Record<string, ApiMethod> = {};
    let initializer: string[] | null = null;

    // Properties
    for (const prop of classDecl.getProperties()) {
      const typeRes = CidlExtractor.cidlType(prop.getType(), true);

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      // Error: invalid property modifier
      const checkModifierRes = checkPropertyModifier(prop);
      if (checkModifierRes.isLeft()) {
        return checkModifierRes;
      }

      if (typeof typeRes.value === "object" && "Inject" in typeRes.value) {
        attributes.push({
          var_name: prop.getName(),
          inject_reference: typeRes.value.Inject,
        });
      }
    }

    // Methods
    for (const m of classDecl.getMethods()) {
      if (m.getName() === "init") {
        // Must not be static
        if (m.isStatic()) {
          return err(ExtractorErrorCode.InvalidServiceInitializer, (e) => {
            e.context = m.getName();
            e.snippet = m.getText();
          });
        }

        const apiMethodRes = this.method(m, HttpVerb.POST); // Verb doesn't matter here
        if (apiMethodRes.isLeft()) {
          return apiMethodRes;
        }

        const method = apiMethodRes.unwrap();

        // Return type must be HttpResult<void> | undefined
        const rt = method.return_type;
        const isVoid =
          rt === "Void" ||
          (typeof rt === "object" &&
            rt !== null &&
            "HttpResult" in rt &&
            rt.HttpResult === "Void");
        if (!isVoid) {
          return err(ExtractorErrorCode.InvalidServiceInitializer, (e) => {
            e.context = m.getName();
            e.snippet = m.getText();
          });
        }

        // All parameters must be injected
        for (const param of m.getParameters()) {
          if (!param.getDecorator(ParameterDecoratorKind.Inject)) {
            return err(ExtractorErrorCode.InvalidServiceInitializer, (e) => {
              e.context = `${m.getName()} parameter ${param.getName()}`;
              e.snippet = m.getText();
            });
          }
        }

        initializer = method.parameters.map(
          (p) => (p.cidl_type as { Inject: string }).Inject,
        );
        continue;
      }

      const httpVerb = m
        .getDecorators()
        .map(getDecoratorName)
        .find((name) =>
          Object.values(HttpVerb).includes(name as HttpVerb),
        ) as HttpVerb;

      if (!httpVerb) {
        continue;
      }

      const res = this.method(m, httpVerb);
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
      initializer,
    });
  }

  private method(
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
      const injected =
        param.getDecorator(ParameterDecoratorKind.Inject) != null;
      const typeRes = CidlExtractor.cidlType(param.getType(), injected);

      // Error: invalid type
      if (typeRes.isLeft()) {
        typeRes.value.snippet = method.getText();
        typeRes.value.context = param.getName();
        return typeRes;
      }

      // Extract any POOs used as parameter types
      const objectName = getObjectName(typeRes.unwrap());
      if (
        objectName &&
        !this.extractedPoos.has(objectName) &&
        !this.modelDecls.has(objectName)
      ) {
        const res = this.poo(
          method.getSourceFile().getClassOrThrow(objectName),
          method.getSourceFile(),
        );

        if (res.isLeft()) {
          res.value.addContext((prev) => `${param.getName()}.${prev}`);
          return res;
        }
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

    // Extract any POOs used as return types
    const objectName = getObjectName(typeRes.unwrap());
    if (
      objectName &&
      !this.extractedPoos.has(objectName) &&
      !this.modelDecls.has(objectName)
    ) {
      const res = this.poo(
        method.getSourceFile().getClassOrThrow(objectName),
        method.getSourceFile(),
      );

      if (res.isLeft()) {
        res.value.addContext((prev) => `returns ${prev}`);
        return res;
      }
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

  private poo(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, null> {
    const name = classDecl.getName()!;
    const attributes: NamedTypedValue[] = [];

    // Error: POOs must be exported
    if (!classDecl.isExported()) {
      return err(ExtractorErrorCode.MissingExport, (e) => {
        e.context = name;
        e.snippet = classDecl.getText();
      });
    }

    for (const prop of classDecl.getProperties()) {
      // Error: invalid property modifier
      const modifierRes = checkPropertyModifier(prop);
      if (modifierRes.isLeft()) {
        return modifierRes;
      }

      const typeRes = CidlExtractor.cidlType(prop.getType());

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      const cidl_type = typeRes.unwrap();

      // Check that the type is an already extracted POO, or a model decl.
      // If not, find the source and extract it as a POO.
      const objectName = getObjectName(cidl_type);
      if (
        objectName &&
        !this.extractedPoos.has(objectName) &&
        !this.modelDecls.has(objectName)
      ) {
        const res = this.poo(
          classDecl.getSourceFile().getClassOrThrow(objectName),
          classDecl.getSourceFile(),
        );

        if (res.isLeft()) {
          res.value.addContext((prev) => `${prop.getName()}.${prev}`);
          return res;
        }
      }

      attributes.push({
        name: prop.getName(),
        cidl_type,
      });
      continue;
    }

    // Mark as extracted
    const poo = {
      name,
      attributes,
      source_path: sourceFile.getFilePath().toString(),
    } satisfies PlainOldObject;
    this.extractedPoos.set(name, poo);

    return Either.right(null);
  }

  // public for tests
  static env(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile,
  ): Either<ExtractorError, WranglerEnv> {
    const vars: Record<string, CidlType> = {};
    let d1_binding: string | undefined = undefined;
    const kv_bindings: string[] = [];
    const r2_bindings: string[] = [];

    for (const prop of classDecl.getProperties()) {
      // Error: invalid property modifier
      const checkModifierRes = checkPropertyModifier(prop);
      if (checkModifierRes.isLeft()) {
        return checkModifierRes;
      }

      // TODO: Support multiple D1 bindings
      if (getTypeText(prop.getType()) === "D1Database") {
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
    R2ObjectBody: "R2Object",
  };

  // public for tests
  static cidlType(
    type: Type,
    inject: boolean = false,
  ): Either<ExtractorError, CidlType> {
    // Any
    if (type.isAny()) {
      return Either.left(new ExtractorError(ExtractorErrorCode.UnknownType));
    }

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
      .replace(/^typeof\s+/, "")
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
    const genericTyText = getTypeText(genericTy);

    const symbolName = unwrappedType.getSymbol()?.getName();
    const aliasName = unwrappedType.getAliasSymbol()?.getName();

    if (aliasName === "DataSourceOf") {
      const [_, genericTyNullable] = unwrapNullable(genericTy);
      const genericTyGenerics = [
        ...genericTy.getAliasTypeArguments(),
        ...genericTy.getTypeArguments(),
      ];

      // Expect DataSourceOf to be of the exact form DataSource<Model>
      if (
        genericTyNullable ||
        genericTy.isUnion() ||
        genericTyGenerics.length > 0
      ) {
        return err(ExtractorErrorCode.UnknownType);
      }

      return Either.right(
        wrapNullable(
          {
            DataSource: genericTyText,
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
        return err(ExtractorErrorCode.UnknownType);
      }

      return Either.right(
        wrapNullable(
          {
            Partial: genericTyText,
          },
          nullable,
        ),
      );
    }

    if (symbolName === ReadableStream.name) {
      return Either.right(wrapNullable("Stream", nullable));
    }

    if (symbolName === "KValue") {
      return wrapGeneric(genericTy, nullable, (inner) => ({ KvObject: inner }));
    }

    if (symbolName === Promise.name || aliasName === "IncludeTree") {
      return wrapGeneric(genericTy, nullable, (inner) => inner);
    }

    if (unwrappedType.isArray()) {
      return wrapGeneric(genericTy, nullable, (inner) => ({ Array: inner }));
    }

    if (symbolName === "HttpResult") {
      return wrapGeneric(genericTy, nullable, (inner) => ({
        HttpResult: inner,
      }));
    }

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
      return res.map((inner) => wrapNullable(wrapper(inner), isNullable));
    }
  }

  /**
   * Mutates the property declaration to add inferred decorators based on naming conventions.
   */
  private inferModelDecorator(
    prop: PropertyDeclaration,
    classDecl: ClassDeclaration,
    cidlType: CidlType,
  ): Either<ExtractorError, void> | void {
    const className = classDecl.getName()!;
    const objectName = getObjectName(cidlType);
    const normalizedPropName = normalizeName(prop.getName());

    // Primary Key
    if (
      normalizedPropName === "id" ||
      normalizedPropName === `${className.toLowerCase()}id`
    ) {
      // Add a primary key decorator
      prop.addDecorator({
        name: PropertyDecoratorKind.PrimaryKey,
        arguments: [],
      });
      return;
    }

    // Foreign Key
    if (normalizedPropName.endsWith("id")) {
      const referencedNavName = prop
        .getName()
        .slice(
          0,
          prop.getName().length - (normalizedPropName.endsWith("_id") ? 3 : 2),
        );

      const oneToOneProperties = classDecl
        .getProperties()
        .filter((p) => p.getName() === referencedNavName);

      if (oneToOneProperties.length > 1) {
        console.warn(`
          Cannot infer ForeignKey relationship due to ambiguity, model ${className}, property ${prop.getName()}
          could match ${oneToOneProperties.map((p) => p.getName()).join(", ")}
          `);
        return;
      }

      // If a one to one property exists with the expected name, use that
      if (oneToOneProperties[0] !== undefined) {
        const oneToOneProperty = oneToOneProperties[0];

        const navModelTypeRes = CidlExtractor.cidlType(
          oneToOneProperty?.getType(),
        );
        if (navModelTypeRes.isLeft()) {
          navModelTypeRes.value.context = prop.getName();
          navModelTypeRes.value.snippet = prop.getText();
          return navModelTypeRes;
        }

        const navModelType = navModelTypeRes.unwrap();
        const objectName = getObjectName(navModelType);

        if (objectName) {
          // Add a foreign key decorator
          prop.addDecorator({
            name: PropertyDecoratorKind.ForeignKey,
            arguments: [objectName],
          });

          return;
        }
      }

      if (objectName !== undefined) {
        const oneToManyClassDecl = this.modelDecls.get(objectName)?.[0];
        const containsOneToManyProp = oneToManyClassDecl
          ?.getProperties()
          .filter((p) => {
            const tyRes = CidlExtractor.cidlType(p.getType());
            if (tyRes.isLeft()) {
              return false;
            }

            const ty = tyRes.unwrap();
            const navObjectName = getObjectName(ty);
            if (navObjectName !== className) {
              return false;
            }

            if (typeof ty === "string" || !("Array" in ty)) {
              return false;
            }

            return true;
          });

        if (containsOneToManyProp) {
          if (containsOneToManyProp.length > 1) {
            console.warn(`
              Cannot infer ForeignKey relationship due to ambiguity, model ${className}, property ${prop.getName()}
              could match ${containsOneToManyProp.map((p) => p.getName()).join(", ")}
              `);
            return;
          }

          // Add a foreign key decorator
          prop.addDecorator({
            name: PropertyDecoratorKind.ForeignKey,
            arguments: [objectName],
          });
          return;
        }
      }
    }

    // One to Many + Many to Many
    if (
      objectName !== undefined &&
      typeof cidlType !== "string" &&
      "Array" in cidlType
    ) {
      const referencedModelDecl = this.modelDecls.get(objectName)?.[0];
      const normalizedModelIdName = `${normalizeName(className)}id`;

      const foreignKeyProps: PropertyDeclaration[] = [];
      const manyToManyProps: PropertyDeclaration[] = [];

      for (const prop of referencedModelDecl?.getProperties() ?? []) {
        const tyRes = CidlExtractor.cidlType(prop.getType());
        if (tyRes.isLeft()) {
          continue;
        }

        const ty = tyRes.unwrap();
        const navObjectName = getObjectName(ty);
        const normalizedPropName = normalizeName(prop.getName());

        if (
          typeof ty !== "string" &&
          "Array" in ty &&
          navObjectName === className
        ) {
          // Many to Many
          manyToManyProps.push(prop);
        } else if (normalizedPropName === normalizedModelIdName) {
          // One to Many
          foreignKeyProps.push(prop);
        }
      }

      if (foreignKeyProps.length > 1) {
        console.warn(`
          Cannot infer OneToMany relationship due to ambiguity, model ${className}, property ${prop.getName()}
          could match ${foreignKeyProps.map((p) => p.getName()).join(", ")}
          `);
        return;
      }

      if (manyToManyProps.length > 1) {
        console.warn(`
          Cannot infer ManyToMany relationship due to ambiguity, model ${className}, property ${prop.getName()}
          could match ${manyToManyProps.map((p) => p.getName()).join(", ")}
          `);
        return;
      }

      const hasForeignKeyProp = foreignKeyProps.at(0);
      const hasManyToManyProp = manyToManyProps.at(0);
      if (hasForeignKeyProp && hasManyToManyProp) {
        console.warn(`
          Cannot infer relationship due to ambiguity, model ${className}, property ${prop.getName()}
          could be OneToMany or ManyToMany
          `);
        return;
      }

      if (hasForeignKeyProp) {
        // Add a one to many decorator
        prop.addDecorator({
          name: PropertyDecoratorKind.OneToMany,
          arguments: [`(m: any) => m.${hasForeignKeyProp.getName()}`],
        });
        return;
      }

      if (hasManyToManyProp) {
        // Add a many to many decorator
        prop.addDecorator({
          name: PropertyDecoratorKind.ManyToMany,
          arguments: [],
        });
        return;
      }
    }

    // One to One
    if (objectName !== undefined) {
      const normalizedPropIdName = `${normalizedPropName}id`;
      const foreignKeyProps = classDecl.getProperties().filter((p) => {
        const norm = normalizeName(p.getName());
        return norm === normalizedPropIdName;
      });

      if (foreignKeyProps.length > 1) {
        console.warn(`
          Cannot infer OneToOne relationship due to ambiguity, model ${className}, property ${prop.getName()}
          could match ${foreignKeyProps.map((p) => p.getName()).join(", ")}
          `);
      }

      if (foreignKeyProps.at(0) !== undefined) {
        const foreignKey = foreignKeyProps[0];
        // Add a one to one decorator
        prop.addDecorator({
          name: PropertyDecoratorKind.OneToOne,
          arguments: [`(_m: any) => m.${foreignKey.getName()}`],
        });
        return;
      }
    }
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

  if (typeof root !== "string" && "Partial" in root) {
    return root["Partial"];
  }

  return undefined;
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

function normalizeName(name: string): string {
  return name.toLowerCase().replace(/_/g, "");
}

function getSelectorPropertyName(
  decorator: Decorator,
): Either<ExtractorError, string> {
  const call = decorator.getCallExpression();
  const selector = call?.getArguments()[0];

  if (!selector?.isKind(SyntaxKind.ArrowFunction)) {
    return err(ExtractorErrorCode.InvalidSelectorSyntax);
  }

  const body = selector.getBody();
  if (!body.isKind(SyntaxKind.PropertyAccessExpression)) {
    return err(ExtractorErrorCode.InvalidSelectorSyntax);
  }

  return Either.right(body.getName());
}

/**
 * Unwraps nullable types from a union type,
 * e.g. `T | null | undefined` becomes `T`.
 * @param ty Type to unwrap
 * @returns A tuple containing the unwrapped type and a boolean indicating if it was nullable.
 */
function unwrapNullable(ty: Type): [Type, boolean] {
  if (!ty.isUnion()) return [ty, false];

  const unions = ty.getUnionTypes();
  const nonNulls = unions.filter((t) => !t.isNull());
  const hasNullable = nonNulls.length < unions.length;

  // Booleans seperate into [null, true, false] from the `getUnionTypes` call
  if (nonNulls.length === 2 && nonNulls.every((t) => t.isBooleanLiteral())) {
    return [nonNulls[0].getApparentType(), hasNullable];
  }

  const stripUndefined = nonNulls.filter((t) => !t.isUndefined());
  return [stripUndefined[0] ?? ty, hasNullable];
}

function getTypeText(type: Type): string {
  return type
    .getText(undefined, TypeFormatFlags.UseAliasDefinedOutsideCurrentScope)
    .replace(/^typeof\s+/, "")
    .split("|")[0]
    .trim();
}
