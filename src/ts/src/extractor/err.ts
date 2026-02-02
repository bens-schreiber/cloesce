export enum ExtractorErrorCode {
  MissingExport,
  InvalidMain,
  UnknownType,
  MultipleGenericType,
  InvalidDataSourceDefinition,
  InvalidPropertyModifier,
  InvalidApiMethodModifier,
  InvalidSelectorSyntax,
  InvalidNavigationProperty,
  TooManyWranglerEnvs,
  InvalidTypescriptSyntax,
  MissingKValue,
  MissingR2ObjectBody,
  InvalidServiceInitializer,
}

const errorInfoMap: Record<
  ExtractorErrorCode,
  { description: string; suggestion: string }
> = {
  [ExtractorErrorCode.MissingExport]: {
    description: "All Cloesce types must be exported.",
    suggestion: "Add `export` to the class definition.",
  },
  [ExtractorErrorCode.InvalidMain]: {
    description: "The main function must follow the expected signature.",
    suggestion:
      "Change to: export default async function main(request: Request, env: WranglerEnv, app: CloesceApp, ctx: ExecutionContext): Promise<Response> {...}",
  },
  [ExtractorErrorCode.UnknownType]: {
    description: "Encountered an unknown or unsupported type",
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
  [ExtractorErrorCode.InvalidPropertyModifier]: {
    description:
      "Attributes can only be public on a Model, Plain Old Object or Wrangler Environment",
    suggestion: "Change the attribute modifier to just `public`",
  },
  [ExtractorErrorCode.InvalidApiMethodModifier]: {
    description:
      "Model methods must be public if they are decorated as GET, POST, PUT, PATCH",
    suggestion: "Change the method modifier to just `public`",
  },
  [ExtractorErrorCode.TooManyWranglerEnvs]: {
    description: "Too many wrangler environments defined in the project",
    suggestion: "Consolidate or remove unused @WranglerEnv's",
  },
  [ExtractorErrorCode.InvalidTypescriptSyntax]: {
    description: "The TypeScript syntax is invalid.",
    suggestion: "Fix the TypeScript syntax errors.",
  },
  [ExtractorErrorCode.MissingKValue]: {
    description: "All KV decorated fields must be of type KValue<T>",
    suggestion: "Change the field type to KValue<T>.",
  },
  [ExtractorErrorCode.MissingR2ObjectBody]: {
    description: "All R2 decorated fields must be of type R2ObjectBody.",
    suggestion: "Change the field type to R2ObjectBody.",
  },
  [ExtractorErrorCode.InvalidSelectorSyntax]: {
    description: "The selector syntax is invalid.",
    suggestion:
      "Selectors should be of the form `N<T>(m => m.property)` where T is a model type and N is OneToOne or OneToMany.",
  },
  [ExtractorErrorCode.InvalidNavigationProperty]: {
    description:
      "A navigation property must be of type T, T | undefined, or T[] where T is a model type.",
    suggestion: "Change the property type to be of the correct form.",
  },
  [ExtractorErrorCode.InvalidServiceInitializer]: {
    description:
      "Service initializers must be instance methods that accept only injected dependencies as parameters and return HttpResult<void> | undefined",
    suggestion: "Update the initializer to match the expected signature.",
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
