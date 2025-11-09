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

type DeepPartialInner<T> = T extends (infer U)[]
  ? DeepPartialInner<U>[]
  : T extends object
    ? { [K in keyof T]?: DeepPartialInner<T[K]> }
    : T | (null extends T ? null : never);

/**
 * Recursively makes all properties of a type optional — including nested objects and arrays.
 *
 * Similar to TypeScript's built-in `Partial<T>`, but applies the transformation deeply across
 * all nested structures. Useful for defining "patch" or "update" objects where only a subset
 * of properties may be provided.
 *
 * **Apart of the Cloesce method grammar**, meaning the type can be apart of method parameters
 * or return types and the generated workers and client API will act accordingly.
 *
 * @template T
 * The target type to make deeply partial.
 *
 * @remarks
 * - **Objects:** All properties become optional, and their values are recursively wrapped in `DeepPartial`.
 * - **Arrays:** Arrays are preserved, but their elements are recursively made partial.
 * - **Scalars:** Primitive values (string, number, boolean, etc.) remain unchanged.
 * - **Nullable types:** If `null` is assignable to the type, it remains allowed.
 *
 * @example
 * ```ts
 * class User {
 *   id: string;
 *   profile: {
 *     name: string;
 *     age: number;
 *   };
 *   tags: string[];
 * }
 *
 * // The resulting type:
 * // {
 * //   id?: string;
 * //   profile?: { name?: string; age?: number };
 * //   tags?: (string | undefined)[];
 * // }
 * type PartialUser = DeepPartial<User>;
 *
 * const patch: PartialUser = {
 *   profile: { age: 30 } // ok
 * };
 * ```
 */
export type DeepPartial<T> = DeepPartialInner<T> & { __brand?: "Partial" };

/**
 * A functional result type representing a computation that can either succeed (`ok: true`)
 * or fail (`ok: false`).
 *
 * `Either<L, R>` is used throughout Cloesce to return structured success/error values
 * instead of throwing exceptions.
 * - When `ok` is `true`, `value` contains the success result of type `R`.
 * - When `ok` is `false`, `value` contains the error information of type `L`.
 *
 * This pattern makes control flow predictable and encourages explicit handling
 * of failure cases.
 *
 * Example:
 * ```ts
 * const result: Either<string, number> = compute();
 *
 * if (!result.ok) {
 *   console.error("Failed:", result.value);
 * } else {
 *   console.log("Success:", result.value);
 * }
 * ```
 */
export type Either<L, R> = { ok: false; value: L } | { ok: true; value: R };

/**
 * Creates a failed `Either` result.
 *
 * Typically used to represent an error condition or unsuccessful operation.
 *
 * @param value The error or failure value to wrap.
 * @returns An `Either` with `ok: false` and the given value.
 */
export function left<L>(value: L): Either<L, never> {
  return { ok: false, value };
}

/**
 * Creates a successful `Either` result.
 *
 * Typically used to represent a successful operation while maintaining
 * a consistent `Either`-based return type.
 *
 * @param value The success value to wrap.
 * @returns An `Either` with `ok: true` and the given value.
 */
export function right<R>(value: R): Either<never, R> {
  return { ok: true, value };
}

/**
 * Represents the result of an HTTP operation in a monadic style.
 *
 * This type provides a uniform way to handle both success and error
 * outcomes of HTTP requests, similar to a `Result` or `Either` monad.
 *
 * It ensures that every HTTP response can be handled in a type-safe,
 * predictable way without throwing exceptions.
 *
 * @template T The type of the successful response data.
 *
 * @property {boolean} ok
 * Indicates whether the HTTP request was successful (`true` for success, `false` for error).
 * This is analogous to `Response.ok` in the Fetch API.
 *
 * @property {number} status
 * The numeric HTTP status code (e.g., 200, 404, 500).
 *
 * @property {T} [data]
 * The parsed response payload, present only when `ok` is `true`.
 *
 * @property {string} [message]
 * An optional human-readable error message or diagnostic information,
 * typically provided when `ok` is `false`.
 *
 * ## Worker APIs
 *
 * HttpResult is a first-class-citizen in the grammar in Cloesce. Methods can return HttpResults
 * which will be serialized on the client api.
 *
 * @example
 * ```ts
 *  bar(): HttpResult<Integer> {
 *    return { ok: false, status: 401, message: "forbidden"}
 *  }
 * ```
 */
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
 * Represents the core middleware container for a Cloesce application.
 *
 * The `CloesceApp` class provides scoped middleware registration and
 * management across three primary levels of execution:
 *
 * 1. **Global Middleware** — Executed before any routing or model resolution occurs.
 * 2. **Model-Level Middleware** — Executed for requests targeting a specific model type.
 * 3. **Method-Level Middleware** — Executed for requests targeting a specific method on a model.
 *
 * When an instance of `CloesceApp` is exported from `app.cloesce.ts`,
 * it becomes the central container that the Cloesce runtime uses to
 * assemble and apply middleware in the correct execution order.
 *
 * ### Middleware Execution Order
 * Middleware is executed in FIFO order per scope. For example:
 * ```ts
 * app.use(Foo, A);
 * app.use(Foo, B);
 * app.use(Foo, C);
 * // Executed in order: A → B → C
 * ```
 *
 * Each middleware function (`MiddlewareFn`) can optionally short-circuit
 * execution by returning a result, in which case subsequent middleware
 * at the same or lower scope will not run.
 *
 * ### Example Usage
 * ```ts
 * import { app } from "cloesce";
 *
 * // Global authentication middleware
 * app.useGlobal((request, env, di) => {
 *   // ... authenticate and inject user
 * });
 *
 * // Model-level authorization
 * app.useModel(User, (user) => user.hasPermissions([UserPermissions.canUseFoo]));
 *
 * // Method-level middleware (e.g., CRUD operation)
 * app.useMethod(Foo, "someMethod", (user) => user.hasPermissions([UserPermissions.canUseFooMethod]));
 * ```
 */
export class CloesceApp {
  public global: MiddlewareFn[] = [];
  public model: Map<string, MiddlewareFn[]> = new Map();
  public method: Map<string, Map<string, MiddlewareFn[]>> = new Map();

  /**
   * Registers a new global middleware function.
   *
   * Global middleware runs before all routing and model resolution.
   * It is the ideal place to perform tasks such as:
   * - Authentication (e.g., JWT verification)
   * - Global request logging
   * - Dependency injection of shared context
   *
   * @param m - The middleware function to register.
   */
  public useGlobal(m: MiddlewareFn) {
    this.global.push(m);
  }

  /**
   * Registers middleware for a specific model type.
   *
   * Model-level middleware runs after all global middleware,
   * but before method-specific middleware. This scope allows
   * logic to be applied consistently across all endpoints
   * associated with a given model (e.g., authorization).
   *
   * @typeParam T - The model type.
   * @param ctor - The model constructor (used to derive its name).
   * @param m - The middleware function to register.
   */
  public useModel<T>(ctor: new () => T, m: MiddlewareFn) {
    if (this.model.has(ctor.name)) {
      this.model.get(ctor.name)!.push(m);
    } else {
      this.model.set(ctor.name, [m]);
    }
  }

  /**
   * Registers middleware for a specific method on a model.
   *
   * Method-level middleware is executed after model middleware,
   * and before the method implementation itself. It can be used for:
   * - Fine-grained permission checks
   * - Custom logging or tracing per endpoint
   *
   * @typeParam T - The model type.
   * @param ctor - The model constructor (used to derive its name).
   * @param method - The method name on the model.
   * @param m - The middleware function to register.
   */
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

export type CrudKind = "SAVE" | "GET" | "LIST";

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
  cruds: CrudKind[];
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
  db_binding: string;
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
