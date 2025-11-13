export enum ExtractorErrorCode {
  MissingExport,
  AppMissingDefaultExport,
  UnknownType,
  MultipleGenericType,
  InvalidDataSourceDefinition,
  InvalidPartialType,
  InvalidIncludeTree,
  InvalidAttributeModifier,
  InvalidApiMethodModifier,
  UnknownNavigationPropertyReference,
  InvalidNavigationPropertyReference,
  MissingNavigationPropertyReference,
  MissingManyToManyUniqueId,
  MissingPrimaryKey,
  MissingDatabaseBinding,
  MissingWranglerEnv,
  TooManyWranglerEnvs,
  MissingFile,
}

const errorInfoMap: Record<
  ExtractorErrorCode,
  { description: string; suggestion: string }
> = {
  [ExtractorErrorCode.MissingExport]: {
    description: "All Cloesce types must be exported.",
    suggestion: "Add `export` to the class definition.",
  },
  [ExtractorErrorCode.AppMissingDefaultExport]: {
    description: "app.cloesce.ts does not export a CloesceApp by default",
    suggestion: "Export an instantiated CloesceApp in app.cloesce.ts",
  },
  [ExtractorErrorCode.UnknownType]: {
    description: "Encountered an unknown or unsupported type",
    suggestion: "Refer to the documentation on valid Cloesce TS types",
  },
  [ExtractorErrorCode.InvalidPartialType]: {
    description: "Partial types must only contain a model or plain old object",
    suggestion: "Refer to the documentation on valid Cloesce TS types",
  },
  [ExtractorErrorCode.MultipleGenericType]: {
    description: "Cloesce does not yet support types with multiple generics",
    suggestion:
      "Simplify your type to use only a single generic parameter, ie Foo<T>",
  },
  [ExtractorErrorCode.InvalidDataSourceDefinition]: {
    description:
      "Data Sources must be explicitly typed as a static Include Tree",
    suggestion:
      "Declare your data source as `static readonly _: IncludeTree<Model>`",
  },
  [ExtractorErrorCode.InvalidIncludeTree]: {
    description: "Invalid Include Tree",
    suggestion:
      "Include trees must only contain references to a model's navigation properties.",
  },
  [ExtractorErrorCode.InvalidAttributeModifier]: {
    description:
      "Attributes can only be public on a Model, Plain Old Object or Wrangler Environment",
    suggestion: "Change the attribute modifier to just `public`",
  },
  [ExtractorErrorCode.InvalidApiMethodModifier]: {
    description:
      "Model methods must be public if they are decorated as GET, POST, PUT, PATCH",
    suggestion: "Change the method modifier to just `public`",
  },
  [ExtractorErrorCode.UnknownNavigationPropertyReference]: {
    description: "Unknown Navigation Property Reference",
    suggestion:
      "Verify that the navigation property reference model exists, or create a model.",
  },
  [ExtractorErrorCode.InvalidNavigationPropertyReference]: {
    description: "Invalid Navigation Property Reference",
    suggestion: "Ensure the navigation property points to a valid model field",
  },
  [ExtractorErrorCode.MissingNavigationPropertyReference]: {
    description: "Missing Navigation Property Reference",
    suggestion:
      "Navigation properties require a foreign key model attribute reference",
  },
  [ExtractorErrorCode.MissingManyToManyUniqueId]: {
    description: "Missing unique id on Many to Many navigation property",
    suggestion:
      "Define a unique identifier field for the Many-to-Many relationship",
  },
  [ExtractorErrorCode.MissingPrimaryKey]: {
    description: "Missing primary key on a model",
    suggestion: "Add a primary key field to your model (e.g., `id: number`)",
  },
  [ExtractorErrorCode.MissingDatabaseBinding]: {
    description: "Missing a database binding in the WranglerEnv definition",
    suggestion: "Add a `D1Database` to your WranglerEnv",
  },
  [ExtractorErrorCode.MissingWranglerEnv]: {
    description: "Missing a wrangler environment definition in the project",
    suggestion: "Add a @WranglerEnv class in your project.",
  },
  [ExtractorErrorCode.TooManyWranglerEnvs]: {
    description: "Too many wrangler environments defined in the project",
    suggestion: "Consolidate or remove unused @WranglerEnv's",
  },
  [ExtractorErrorCode.MissingFile]: {
    description: "A specified input file could not be found",
    suggestion: "Verify the input file path is correct",
  },
};

export function getErrorInfo(code: ExtractorErrorCode) {
  return errorInfoMap[code];
}

export class ExtractorError {
  context?: string;
  snippet?: string;

  constructor(public code: ExtractorErrorCode) {}

  addContext(fn: (val: string | undefined) => string | undefined) {
    this.context = fn(this.context ?? "");
  }
}
