/// A partial type whose object keys may be partial as well
export type DeepPartial<T> = (T extends (infer U)[]
  ? DeepPartial<U>[] // handle arrays specially
  : T extends object
    ? { [K in keyof T]?: DeepPartial<T[K]> }
    : T) & { __brand?: "Partial" };

export type Either<L, R> = { ok: false; value: L } | { ok: true; value: R };
export function left<L>(value: L): Either<L, never> {
  return { ok: false, value };
}
export function right<R>(value: R): Either<never, R> {
  return { ok: true, value };
}

export enum ExtractorErrorCode {
  MissingExport,
  AppMissingDefaultExport,
  UnknownType,
  MultipleGenericType,
  DataSourceMissingStatic,
  InvalidPartialType,
  InvalidIncludeTree,
  InvalidAttributeModifier,
  InvalidApiMethodModifier,
  UnknownNavigationPropertyReference,
  InvalidNavigationPropertyReference,
  MissingNavigationPropertyReference,
  MissingManyToManyUniqueId,
  MissingPrimaryKey,
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
  [ExtractorErrorCode.DataSourceMissingStatic]: {
    description: "Data Sources must be declared as static",
    suggestion: "Declare your data source as `static readonly`",
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

export type HttpResult<T = unknown> = {
  ok: boolean;
  status: number;
  data?: T;
  message?: string;
};

/**
 * Dependency injection container, mapping an object type name to an instance of that object.
 *
 * Comes with the WranglerEnv and Request by default.
 */
export type InstanceRegistry = Map<string, any>;
export type MiddlewareFn = (
  request: Request,
  env: any,
  ir: InstanceRegistry,
) => Promise<HttpResult | undefined>;

export type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? (K extends string ? K : never) : never;
}[keyof T];

/**
 * A container for middleware. If an instance is exported from `app.cloesce.ts`, it will be used in the
 * appropriate location, with global middleware happening before any routing occurs.
 */
export class CloesceApp {
  public global: MiddlewareFn[] = [];
  public model: Map<string, MiddlewareFn[]> = new Map();
  public method: Map<string, Map<string, MiddlewareFn[]>> = new Map();

  public useGlobal(m: MiddlewareFn) {
    this.global.push(m);
  }

  public useModel<T>(ctor: new () => T, m: MiddlewareFn) {
    if (this.model.has(ctor.name)) {
      this.model.get(ctor.name)!.push(m);
    } else {
      this.model.set(ctor.name, [m]);
    }
  }

  public useMethod<T>(
    ctor: new () => T,
    method: KeysOfType<T, (...args: any) => any>,
    m: MiddlewareFn,
  ) {
    if (!this.method.has(ctor.name)) {
      this.method.set(ctor.name, new Map());
    }

    const methods = this.method.get(ctor.name)!;
    if (!methods.has(method)) {
      methods.set(method, []);
    }

    methods.get(method)!.push(m);
  }
}

export type CrudKind = "POST" | "GET" | "LIST";

export type CidlType =
  | "Void"
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | "DateIso"
  | "Boolean"
  | { DataSource: string }
  | { Inject: string }
  | { Object: string }
  | { Partial: string }
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
  nav: NavigationProperty,
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

export const NO_DATA_SOURCE = "none";
export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

export interface WranglerEnv {
  name: string;
  source_path: string;
}

export interface CloesceAst {
  version: string;
  project_name: string;
  language: "TypeScript";
  wrangler_env: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  app_source: string | null;
}
