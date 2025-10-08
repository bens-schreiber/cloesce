export type Either<L, R> = { ok: false; value: L } | { ok: true; value: R };
export function left<L>(value: L): Either<L, never> {
  return { ok: false, value };
}
export function right<R>(value: R): Either<never, R> {
  return { ok: true, value };
}

export enum ExtractorErrorCode {
  UnknownType,
  MultipleGenericType,
  InvalidIncludeTree,
  UnknownNavigationPropertyReference,
  InvalidNavigationPropertyReference,
  MissingNavigationPropertyReference,
  MissingManyToManyUniqueId,
  MissingPrimaryKey,
  MissingWranglerEnv,
  TooManyWranglerEnvs,
  TooManyMiddlewares,
  MissingMiddlewareMethod,
}

const errorInfoMap: Record<
  ExtractorErrorCode,
  { description: string; suggestion: string }
> = {
  [ExtractorErrorCode.UnknownType]: {
    description: "Encountered an unknown or unsupported type",
    suggestion: "Refer to the documentation on valid Cloesce TS types",
  },
  [ExtractorErrorCode.MultipleGenericType]: {
    description: "Cloesce does not yet support types with multiple generics",
    suggestion:
      "Simplify your type to use only a single generic parameter, ie Foo<T>",
  },
  [ExtractorErrorCode.InvalidIncludeTree]: {
    description: "Invalid Include Tree",
    suggestion:
      "Include trees must only contain references to a model's navigation properties.",
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
  [ExtractorErrorCode.MissingWranglerEnv]: {
    description: "Missing a wrangler environment definition in the project",
    suggestion: "Add a @WranglerEnv class in your project.",
  },
  [ExtractorErrorCode.TooManyWranglerEnvs]: {
    description: "Too many wrangler environments defined in the project",
    suggestion: "Consolidate or remove unused @WranglerEnv's",
  },
  [ExtractorErrorCode.TooManyMiddlewares]: {
    description: "Too many middleware classes defined in the project",
    suggestion: "Only one @Middleware class is allowed per project",
  },
  [ExtractorErrorCode.MissingMiddlewareMethod]: {
    description: "Middleware class must have a 'handle' method",
    suggestion: "Add a 'handle' method to your @Middleware class",
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

export type HttpResult<T = unknown> = {
  ok: boolean;
  status: number;
  data?: T;
  message?: string;
};

export type CidlType =
  | "Void"
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | { Inject: string }
  | { Object: string }
  | { Nullable: CidlType }
  | { Array: CidlType }
  | { HttpResult: CidlType };

export function isNullableType(ty: CidlType): boolean {
  return typeof ty === "object" && ty !== null && "Nullable" in ty;
}

export enum HttpVerb {
  GET = "GET",
  POST = "POST",
  PUT = "PUT",
  PATCH = "PATCH",
  DELETE = "DELETE",
}

export interface NamedTypedValue {
  name: string;
  cidl_type: CidlType;
}

export interface ModelAttribute {
  value: NamedTypedValue;
  foreign_key_reference: string | null;
}

export interface ModelMethod {
  name: string;
  is_static: boolean;
  http_verb: HttpVerb;
  return_type: CidlType | null;
  parameters: NamedTypedValue[];
}

export type NavigationPropertyKind =
  | { OneToOne: { reference: string } }
  | { OneToMany: { reference: string } }
  | { ManyToMany: { unique_id: string } };

export interface NavigationProperty {
  var_name: string;
  model_name: string;
  kind: NavigationPropertyKind;
}

export function getNavigationPropertyCidlType(
  nav: NavigationProperty
): CidlType {
  return "OneToOne" in nav.kind
    ? { Object: nav.model_name }
    : { Array: { Object: nav.model_name } };
}

export interface Model {
  name: string;
  primary_key: NamedTypedValue;
  attributes: ModelAttribute[];
  navigation_properties: NavigationProperty[];
  methods: Record<string, ModelMethod>;
  data_sources: Record<string, DataSource>;
  source_path: string;
}

export interface PlainOldObject {
  name: string;
  attributes: NamedTypedValue[];
  source_path: string;
}

export interface CidlIncludeTree {
  [key: string]: CidlIncludeTree;
}

export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

export interface WranglerEnv {
  name: string;
  source_path: string;
}

export interface MiddlewareMethod {
  name: string;
  parameters: NamedTypedValue[];
}

export interface Middleware {
  class_name: string;
  method: MiddlewareMethod;
  source_path: string;
}

export interface CloesceAst {
  version: string;
  project_name: string;
  language: "TypeScript";
  wrangler_env: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  middleware: Middleware | null;
}
