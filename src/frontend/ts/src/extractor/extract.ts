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
  Scope,
} from "ts-morph";

import {
  CidlIncludeTree,
  CloesceAst,
  CidlType,
  DataSource,
  HttpVerb,
  Model,
  ModelAttribute,
  ApiMethod,
  NamedTypedValue,
  NavigationProperty,
  WranglerEnv,
  PlainOldObject,
  CrudKind,
  Service,
  defaultMediaType,
} from "../ast.js";
import { TypeFormatFlags } from "typescript";
import { ExtractorError, ExtractorErrorCode } from "./err.js";
import { Either } from "../ui/common.js";

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
  Service = "Service",
  CRUD = "CRUD",
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
    const services: Record<string, Service> = {};
    let app_source: string | null = null;

    for (const sourceFile of project.getSourceFiles()) {
      if (
        sourceFile.getBaseName() === "app.cloesce.ts" ||
        sourceFile.getBaseName() === "seed__app.cloesce.ts" // hardcoding for tests
      ) {
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

        if (hasDecorator(classDecl, ClassDecoratorKind.D1)) {
          if (!classDecl.isExported()) return notExportedErr;
          const result = CidlExtractor.model(classDecl, sourceFile);

          // Error: propogate from models
          if (result.isLeft()) {
            result.value.addContext((prev) => `${classDecl.getName()}.${prev}`);
            return result;
          }

          const model = result.unwrap();
          models[model.name] = model;
          continue;
        }

        if (hasDecorator(classDecl, ClassDecoratorKind.Service)) {
          if (!classDecl.isExported()) return notExportedErr;
          const result = CidlExtractor.service(classDecl, sourceFile);

          // Error: propogate from service
          if (result.isLeft()) {
            result.value.addContext((prev) => `${classDecl.getName()}.${prev}`);
            return result;
          }

          const service = result.unwrap();
          services[service.name] = service;
          continue;
        }

        if (hasDecorator(classDecl, ClassDecoratorKind.PlainOldObject)) {
          if (!classDecl.isExported()) return notExportedErr;
          const result = CidlExtractor.poo(classDecl, sourceFile);

          // Error: propogate from models
          if (result.isLeft()) {
            result.value.addContext((prev) => `${classDecl.getName()}.${prev}`);
            return result;
          }
          poos[result.unwrap().name] = result.unwrap();
          continue;
        }

        if (hasDecorator(classDecl, ClassDecoratorKind.WranglerEnv)) {
          // Error: invalid attribute modifier
          for (const prop of classDecl.getProperties()) {
            const modifierRes = checkAttributeModifier(prop);
            if (modifierRes.isLeft()) {
              return modifierRes;
            }
          }

          const result = CidlExtractor.env(classDecl, sourceFile);
          if (result.isLeft()) {
            return result;
          }

          wranglerEnvs.push(result.unwrap());
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

    return Either.right({
      version: this.version,
      project_name: this.projectName,
      language: "TypeScript",
      wrangler_env: wranglerEnvs[0],
      models,
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
        TypeFormatFlags.UseAliasDefinedOutsideCurrentScope
      );
    };

    const typeText = getTypeText();
    if (typeText === "CloesceApp") {
      return Either.right(sourceFile.getFilePath().toString());
    }

    return err(ExtractorErrorCode.AppMissingDefaultExport);
  }

  static model(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile
  ): Either<ExtractorError, Model> {
    const name = classDecl.getName()!;
    const attributes: ModelAttribute[] = [];
    const navigation_properties: NavigationProperty[] = [];
    const data_sources: Record<string, DataSource> = {};
    const methods: Record<string, ApiMethod> = {};
    const cruds: Set<CrudKind> = new Set<CrudKind>();
    let primary_key: NamedTypedValue | undefined = undefined;

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

      const checkModifierRes = checkAttributeModifier(prop);

      // No decorators means this is a standard attribute
      if (decorators.length === 0) {
        // Error: invalid attribute modifier
        if (checkModifierRes.isLeft()) {
          return checkModifierRes;
        }

        const cidl_type = typeRes.unwrap();
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
      const decoratorName = getDecoratorName(decorator);

      // Error: invalid attribute modifier
      if (
        checkModifierRes.isLeft() &&
        decoratorName !== AttributeDecoratorKind.DataSource
      ) {
        return checkModifierRes;
      }

      // Process decorator
      const cidl_type = typeRes.unwrap();
      switch (decoratorName) {
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

          navigation_properties.push({
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

          navigation_properties.push({
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

          navigation_properties.push({
            var_name: prop.getName(),
            model_name,
            kind: { ManyToMany: { unique_id } },
          });
          break;
        }
        case AttributeDecoratorKind.DataSource: {
          const isIncludeTree =
            prop
              .getType()
              .getText(
                undefined,
                TypeFormatFlags.UseAliasDefinedOutsideCurrentScope
              ) === `IncludeTree<${name}>`;

          // Error: data sources must be static include trees
          if (!prop.isStatic() || !isIncludeTree) {
            return err(ExtractorErrorCode.InvalidDataSourceDefinition, (e) => {
              e.snippet = prop.getText();
              e.context = prop.getName();
            });
          }

          const initializer = prop.getInitializer();

          const treeRes = CidlExtractor.includeTree(
            initializer,
            classDecl,
            sourceFile
          );

          if (treeRes.isLeft()) {
            treeRes.value.addContext((prev) => `${prop.getName()} ${prev}`);
            treeRes.value.snippet = prop.getText();
            return treeRes;
          }

          data_sources[prop.getName()] = {
            name: prop.getName(),
            tree: treeRes.unwrap(),
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
      const httpVerb = m
        .getDecorators()
        .map((d) => getDecoratorName(d))
        .find((name) =>
          Object.values(HttpVerb).includes(name as HttpVerb)
        ) as HttpVerb;

      if (!httpVerb) {
        continue;
      }

      const result = CidlExtractor.modelMethod(name, m, httpVerb);
      if (result.isLeft()) {
        result.value.addContext((prev) => `${m.getName()} ${prev}`);
        return result;
      }
      methods[result.unwrap().name] = result.unwrap();
    }

    return Either.right({
      name,
      attributes,
      primary_key,
      navigation_properties,
      methods,
      data_sources,
      cruds: Array.from(cruds).sort(),
      source_path: sourceFile.getFilePath().toString(),
    });
  }

  static modelMethod(
    modelName: string,
    method: MethodDeclaration,
    verb: HttpVerb
  ): Either<ExtractorError, ApiMethod> {
    // Error: invalid method scope, must be public
    if (method.getScope() != Scope.Public) {
      return err(ExtractorErrorCode.InvalidApiMethodModifier, (e) => {
        e.context = method.getName();
        e.snippet = method.getText();
      });
    }

    let needsDataSource = !method.isStatic();
    const parameters: NamedTypedValue[] = [];

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
      if (typeof typeRes.value !== "string" && "DataSource" in typeRes.value) {
        needsDataSource = false;
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

    // Sugaring: add data source
    if (needsDataSource) {
      parameters.push({
        name: "__dataSource",
        cidl_type: { DataSource: modelName },
      });
    }

    return Either.right({
      name: method.getName(),
      is_static: method.isStatic(),
      http_verb: verb,
      return_media: defaultMediaType(),
      return_type: typeRes.unwrap(),
      parameters_media: defaultMediaType(),
      parameters,
    });
  }

  static service(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile
  ): Either<ExtractorError, Service> {
    const attributes = [];
    const methods: Record<string, ApiMethod> = {};

    // Attributes
    for (const prop of classDecl.getProperties()) {
      const typeRes = CidlExtractor.cidlType(prop.getType(), true);

      // Error: invalid property type
      if (typeRes.isLeft()) {
        typeRes.value.context = prop.getName();
        typeRes.value.snippet = prop.getText();
        return typeRes;
      }

      if (typeof typeRes.value === "string" || !("Inject" in typeRes.value)) {
        return err(ExtractorErrorCode.InvalidServiceAttribute, (e) => {
          e.context = prop.getName();
          e.snippet = prop.getText();
        });
      }

      const checkModifierRes = checkAttributeModifier(prop);
      if (checkModifierRes.isLeft()) {
        return checkModifierRes;
      }

      attributes.push({
        var_name: prop.getName(),
        injected: typeRes.value.Inject,
      });
    }

    // Methods
    for (const m of classDecl.getMethods()) {
      const httpVerb = m
        .getDecorators()
        .map((d) => getDecoratorName(d))
        .find((name) =>
          Object.values(HttpVerb).includes(name as HttpVerb)
        ) as HttpVerb;

      if (!httpVerb) {
        continue;
      }

      const res = CidlExtractor.serviceMethod(m, httpVerb);
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

  static serviceMethod(
    method: MethodDeclaration,
    verb: HttpVerb
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

  static poo(
    classDecl: ClassDeclaration,
    sourceFile: SourceFile
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

      // Error: invalid attribute modifier
      const modifierRes = checkAttributeModifier(prop);
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
    sourceFile: SourceFile
  ): Either<ExtractorError, WranglerEnv> {
    const vars: Record<string, CidlType> = {};
    let binding;

    for (const prop of classDecl.getProperties()) {
      if (
        prop
          .getType()
          .getText(
            undefined,
            TypeFormatFlags.UseAliasDefinedOutsideCurrentScope
          ) === "D1Database"
      ) {
        binding = prop.getName();
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

    if (!binding) {
      return err(ExtractorErrorCode.MissingDatabaseBinding);
    }

    return Either.right({
      name: classDecl.getName()!,
      source_path: sourceFile.getFilePath().toString(),
      db_binding: binding,
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
    Blob: "Blob",
  };

  static cidlType(
    type: Type,
    inject: boolean = false
  ): Either<ExtractorError, CidlType> {
    // Void
    if (type.isVoid()) {
      return Either.right("Void");
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
              TypeFormatFlags.UseAliasDefinedOutsideCurrentScope
            ),
          },
          nullable
        )
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
                TypeFormatFlags.UseAliasDefinedOutsideCurrentScope
              )
              .split("|")[0]
              .trim(),
          },
          nullable
        )
      );
    }

    // Ignore
    if (symbolName === "Promise" || aliasName === "IncludeTree") {
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

  static includeTree(
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
      if (typeRes.isLeft()) {
        typeRes.value.snippet = navProp.getText();
        typeRes.value.context = prop.getName();
        return typeRes;
      }

      // Error: invalid referenced nav prop type
      const cidl_type = typeRes.unwrap();
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
          if (treeRes.isLeft()) {
            treeRes.value.snippet = expr.getText();
            return treeRes;
          }

          nestedTree = treeRes.unwrap();
        }
      }

      result[navProp.getName()] = nestedTree;
    }

    return Either.right(result);
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
  return Either.left(e);
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
          : a.getText()) as CrudKind
      );
    }
  }
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

function checkAttributeModifier(
  prop: PropertyDeclaration
): Either<ExtractorError, null> {
  // Error: attributes must be just 'public'
  if (prop.getScope() != Scope.Public || prop.isReadonly() || prop.isStatic()) {
    return err(ExtractorErrorCode.InvalidAttributeModifier, (e) => {
      e.context = prop.getName();
      e.snippet = prop.getText();
    });
  }
  return Either.right(null);
}
