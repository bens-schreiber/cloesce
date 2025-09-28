import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
import {
  HttpResult,
  Either,
  Model,
  ModelMethod,
  MetaCidl,
  left,
  CidlType,
  right,
  MetaModel,
  CidlSpec,
} from "./common.js";

/**
 * Users will create Cloesce models, which have metadata for them in the CIDL.
 * For TypeScript's purposes, these models can be anything. We can assume any
 * `UserDefinedModel` has been verified by the compiler.
 */
type UserDefinedModel = any;
type InstantiatedUserDefinedModel = object;

/**
 * Given a request, this represents a map of each body / url  param name to
 * its actual value. Unknown, as the a request can be anything.
 */
type RequestParamMap = Record<string, unknown>;

/**
 * A map of class names to their appropriate constructor, which is generated
 * by the Cloesce compiler.
 */
type ConstructorRegistry = Record<string, new () => UserDefinedModel>;

/**
 * Singleton instance of the MetaCidl and Constructor Registry.
 * These values are guaranteed to never change throughout a programs lifetime.
 */
class MetaContainer {
  private static instance: MetaContainer | undefined;
  private constructor(
    public readonly cidl: MetaCidl,
    public readonly constructorRegistry: ConstructorRegistry,
  ) {}

  static init(
    rawCidl: CidlSpec,
    constructorRegistry: ConstructorRegistry,
  ): MetaContainer {
    if (!this.instance) {
      this.instance = new MetaContainer(
        {
          ...rawCidl,
          models: Object.fromEntries(
            rawCidl.models.map((m) => [
              m.name,
              {
                ...m,
                methods: Object.fromEntries(m.methods.map((x) => [x.name, x])),
              },
            ]),
          ),
        },
        constructorRegistry,
      );
    }
    return this.instance;
  }

  static get(): MetaContainer {
    if (!this.instance) {
      throw new Error("Cloesce not initialized. Call Cloesce.init() first.");
    }
    return this.instance;
  }
}

/**
 * Creates model instances given a properly formatted SQL record
 * (either a foreign-key-less model or derived from a Cloesce generated view)
 * @param ctor The type of the model
 * @param records SQL records
 * @param includeTree The include tree to use when parsing the records
 * @returns
 */
export function modelsFromSql<T>(
  ctor: new () => UserDefinedModel,
  records: Record<string, any>[],
  includeTree: Record<string, UserDefinedModel>,
): T[] {
  const { cidl, constructorRegistry } = MetaContainer.get();
  return _modelsFromSql(
    ctor.name,
    cidl,
    constructorRegistry,
    records,
    includeTree,
  ) as T[];
}

/**
 * Cloesce entry point. Given a request, undergoes routing, validating,
 * hydrating, and method dispatch.
 * @param rawCidl The full unfiltered cidl
 * @param constructorRegistry A mapping of user defined class names to their respective constructor
 * @param request An incoming request to the workers server
 * @param api_route The url's path to the api, e.g. api/v1/fooapi/
 * @param d1 Cloudflare D1 instance
 * @returns A Response with an `HttpResult` JSON body.
 */
export async function cloesce(
  rawCidl: CidlSpec,
  constructorRegistry: ConstructorRegistry,
  request: Request,
  api_route: string,
  d1: D1Database,
): Promise<Response> {
  const { cidl } = MetaContainer.init(rawCidl, constructorRegistry);

  // Match the route to a model method
  let route = matchRoute(request, api_route, cidl);
  if (!route.ok) {
    return toResponse(route.value);
  }
  let { modelMeta, methodMeta, id } = route.value;

  // Validate request body to the model method
  let isValidRequest = await validateRequest(request, cidl, methodMeta, id);
  if (!isValidRequest.ok) {
    return toResponse(isValidRequest.value);
  }
  let requestParamMap = isValidRequest.value;

  // Instantatiate the model
  let instance: object;
  if (methodMeta.is_static) {
    instance = constructorRegistry[modelMeta.name];
  } else {
    let successfulModel = await hydrateModel(
      cidl,
      modelMeta,
      constructorRegistry,
      d1,
      id!,
    );

    if (!successfulModel.ok) {
      return toResponse(successfulModel.value);
    }

    instance = successfulModel.value;
  }

  // Dispatch a method on the model and return the result
  return toResponse(
    await methodDispatch(instance, methodMeta, requestParamMap, d1),
  );
}

// TODO: In the previous version, we would walk a generated trie
// This is more hardcode-y, and I'm not sure it will hold up to time.
/**
 * Matches a request to a method on a model.
 * @param api_route The route from the domain to the actual API, ie https://foo.com/route/to/api => route/to/api/
 * @returns 404 or a `MatchedRoute`
 */
function matchRoute(
  request: Request,
  api_route: string,
  cidl: MetaCidl,
): Either<HttpResult, MatchedRoute> {
  const url = new URL(request.url);

  const notFound = (e: string) =>
    left(error_state(404, `Path not found: ${e} ${url.pathname}`));

  const routeParts = url.pathname
    .slice(api_route.length)
    .split("/")
    .filter(Boolean);

  if (routeParts.length < 2) {
    return notFound("Expected /model/method or /model/:id/method");
  }

  // Attempt to extract from routeParts
  const modelName = routeParts[0];
  const methodName = routeParts[routeParts.length - 1];
  const id = routeParts.length === 3 ? routeParts[1] : null;

  const modelMeta = cidl.models[modelName];
  if (!modelMeta) {
    return notFound(`Unknown model ${modelName}`);
  }

  const methodMeta = modelMeta.methods[methodName];
  if (!methodMeta) {
    return notFound(`Unknown method ${modelName}.${methodName}`);
  }

  if (request.method !== methodMeta.http_verb) {
    return notFound("Unmatched HTTP method");
  }

  return right({
    modelMeta,
    methodMeta,
    id,
  });
}

/**
 * Validates the request's body/search params against a ModelMethod
 * @returns 400 or a `RequestParamMap` consisting of each parameters name mapped to its value
 */
async function validateRequest(
  request: Request,
  cidl: MetaCidl,
  methodMeta: ModelMethod,
  id: string | null,
): Promise<Either<HttpResult, RequestParamMap>> {
  // Error state: any missing parameter, body, or malformed input will exit with 400.
  const invalid_request = (e: string) =>
    left(error_state(400, `Invalid Request Body: ${e}`));

  if (!methodMeta.is_static && id == null) {
    return invalid_request("Id's are required for instantiated methods.");
  }

  // Filter out any injected parameters that will not be passed
  // by the query.
  let requiredParams = methodMeta.parameters.filter(
    (p) => p.cidl_type !== "D1Database",
  );

  let requestBodyMap: RequestParamMap;
  if (methodMeta.http_verb === "GET") {
    const url = new URL(request.url);
    requestBodyMap = Object.fromEntries(url.searchParams.entries());
  } else {
    try {
      requestBodyMap = await request.json();
    } catch {
      return invalid_request("Could not retrieve JSON body.");
    }
  }

  // Ensure all required params exist
  if (!requiredParams.every((p) => p.name in requestBodyMap)) {
    return invalid_request(`Missing parameters.`);
  }

  // Validate all parameters type
  for (const p of requiredParams) {
    const value = requestBodyMap[p.name];
    if (!validateCidlType(value, p.cidl_type, cidl, p.nullable)) {
      return invalid_request("Invalid parameters.");
    }
  }

  return right(requestBodyMap);

  function validateCidlType(
    value: unknown,
    cidlType: CidlType,
    cidl: MetaCidl,
    nullable: boolean,
  ): boolean {
    if (value === undefined) return false;

    // TODO: consequences of null checking like this? 'null' is passed in
    // as a string for GET requests...
    if (value == null || value === "null") return nullable;

    // Handle primitive string types with switch
    if (typeof cidlType === "string") {
      switch (cidlType) {
        case "Integer":
          return Number.isInteger(Number(value));
        case "Real":
          return !Number.isNaN(Number(value));
        case "Text":
          return typeof value === "string";
        case "Blob":
          return value instanceof Blob || value instanceof ArrayBuffer;
        default:
          return false;
      }
    }

    // Handle object types
    if ("Model" in cidlType) {
      const model = cidl.models[cidlType.Model];
      if (!model || typeof value !== "object") return false;
      const obj = value as Record<string, unknown>;

      // Validate attributes
      if (
        !model.attributes.every((attr) =>
          validateCidlType(
            obj[attr.value.name],
            attr.value.cidl_type,
            cidl,
            attr.value.nullable,
          ),
        )
      ) {
        return false;
      }

      // Validate navigation properties (optional)
      return model.navigation_properties.every((nav) => {
        const navValue = obj[nav.value.name];
        return (
          navValue == null ||
          validateCidlType(
            navValue,
            nav.value.cidl_type,
            cidl,
            nav.value.nullable,
          )
        );
      });
    }

    if ("Array" in cidlType) {
      return (
        Array.isArray(value) &&
        value.every((v) => validateCidlType(v, cidlType.Array, cidl, nullable))
      );
    }

    if ("HttpResult" in cidlType) {
      if (value === null) return cidlType.HttpResult === null;
      if (cidlType.HttpResult === null) return false;
      return validateCidlType(value, cidlType.HttpResult, cidl, nullable);
    }

    return false;
  }
}

/**
 * Queries D1 for a particular model's ID, then transforms the SQL column output into
 * an instance of a model using the provided include tree and metadata as a guide.
 * @returns 404 if no record was found for the provided ID
 * @returns 500 if the D1 database is not synced with Cloesce and yields an error
 * @returns The instantiated model on success
 */
async function hydrateModel(
  cidl: MetaCidl,
  modelMeta: MetaModel,
  constructorRegistry: ConstructorRegistry,
  d1: D1Database,
  id: string,
): Promise<Either<HttpResult, object>> {
  // Error state: If the D1 database has been tweaked outside of Cloesce
  // resulting in a malformed query, exit with a 500.
  const malformedQuery = (e: any) =>
    left(
      error_state(
        500,
        `Error in hydration query, is the database out of sync with the backend?: ${e instanceof Error ? e.message : String(e)}`,
      ),
    );

  // Error state: If no record is found for the id, return a 404
  const missingRecord = left(error_state(404, "Record not found"));

  // TODO: We are assuming defalt DS for now
  const pk = modelMeta.primary_key.name;
  const hasDataSources = modelMeta.data_sources.length > 0;
  const query = hasDataSources
    ? `SELECT * FROM ${modelMeta.name}_default WHERE ${modelMeta.name}_${pk} = ?`
    : `SELECT * FROM ${modelMeta.name} WHERE ${pk} = ?`;

  // Query DB
  let records;
  try {
    records = await d1.prepare(query).bind(id).run();
    if (!records) {
      return missingRecord;
    }
  } catch (e) {
    return malformedQuery(e);
  }

  // TODO: assuming default DS again
  if (hasDataSources) {
    let includeTree: any = new constructorRegistry[modelMeta.name]().default;

    let models: object[] = _modelsFromSql(
      modelMeta.name,
      cidl,
      constructorRegistry,
      records.results,
      includeTree,
    );
    return right(models[0]);
  }

  return right(
    Object.assign(
      new constructorRegistry[modelMeta.name](),
      records.results[0],
    ),
  );
}

/**
 * Calls a method on a model given a list of parameters.
 * @returns 500 on an uncaught client error, 200 with a result body on success
 */
async function methodDispatch(
  instance: InstantiatedUserDefinedModel,
  methodMeta: ModelMethod,
  params: Record<string, unknown>,
  d1: D1Database,
): Promise<HttpResult<unknown>> {
  // Error state: Client code ran into an uncaught exception.
  const uncaughtException = (e: any) =>
    error_state(500, `${e instanceof Error ? e.message : String(e)}`);

  // For now, the only injected dependency is d1, so we will assume that is what this is
  const paramArray = methodMeta.parameters.map((p) =>
    params[p.name] == undefined ? d1 : params[p.name],
  );

  // Ensure the result is always some HttpResult
  const resultWrapper = (res: any): HttpResult<unknown> => {
    const rt = methodMeta.return_type;

    if (rt === null) {
      return { ok: true, status: 200 };
    }

    if (typeof rt === "object" && rt !== null && "HttpResult" in rt) {
      return res as HttpResult<unknown>;
    }

    return { ok: true, status: 200, data: res };
  };

  try {
    return resultWrapper(
      await (instance as any)[methodMeta.name](...paramArray),
    );
  } catch (e) {
    return uncaughtException(e);
  }
}

/**
 * Actual implementation of sql to model mapping.
 *
 * TODO: If we don't want to write this in every language, would it be possible to create a
 * single WASM binary for this method?
 *
 * @throws generic errors if the metadata is missing some value
 */
function _modelsFromSql(
  modelName: string,
  cidl: MetaCidl,
  constructorRegistry: ConstructorRegistry,
  records: Record<string, any>[],
  includeTree: Record<string, UserDefinedModel>,
): InstantiatedUserDefinedModel[] {
  if (!records.length) return [];

  const modelMeta = cidl.models[modelName];
  if (!modelMeta) throw new Error(`Model ${modelName} not found in CIDL`);

  const pk = modelMeta.primary_key;
  if (!pk) throw new Error(`Primary key not found for ${modelName}`);

  const pkName = pk.name;
  const itemsById: Record<string, any> = {};
  const seenNestedIds: Record<string, Set<string>> = {};

  // Create all root entities with initialized arrays
  for (const row of records) {
    const isPrefixed = Object.keys(row).some((k) =>
      k.startsWith(`${modelName}_`),
    );
    const rootId = String(isPrefixed ? row[`${modelName}_id`] : row[pkName]);

    if (!itemsById[rootId]) {
      const instance = new constructorRegistry[modelName]();

      // Assign primary key
      instance[modelMeta.primary_key.name] = getCol(
        modelMeta,
        modelMeta.primary_key.name,
        row,
        isPrefixed,
      );

      // Assign scalar attributes
      for (const attr of modelMeta.attributes) {
        instance[attr.value.name] = getCol(
          modelMeta,
          attr.value.name,
          row,
          isPrefixed,
        );
      }

      // Initialize all array navigation properties
      for (const nav of modelMeta.navigation_properties) {
        const navCidlType = nav.value.cidl_type;
        if (isCidlArray(navCidlType)) {
          instance[nav.value.name] = [];
        }
      }

      itemsById[rootId] = instance;
    }
  }

  // Populate navigation properties
  for (const row of records) {
    const isPrefixed = Object.keys(row).some((k) =>
      k.startsWith(`${modelName}_`),
    );
    const rootId = String(isPrefixed ? row[`${modelName}_id`] : row[pkName]);
    const existing = itemsById[rootId];

    // Process navigation properties
    for (const nav of modelMeta.navigation_properties) {
      const navName = nav.value.name;
      const navCidlType = nav.value.cidl_type;
      let navModelName: string | undefined;

      if (isCidlArray(navCidlType)) {
        if (isCidlModel(navCidlType.Array)) {
          navModelName = navCidlType.Array.Model;
        }
      } else if (isCidlModel(navCidlType)) {
        navModelName = navCidlType.Model;
      }

      if (!navModelName) continue;

      const navMeta = cidl.models[navModelName];
      if (!navMeta) continue;

      const nestedPk = navMeta.primary_key.name;
      const nestedId = row[`${navMeta.name}_${nestedPk}`];
      const isArray = isCidlArray(navCidlType);

      // Only process if we're supposed to include this navigation property AND there's data
      if (includeTree?.[navName] && nestedId != null) {
        const nestedObj = buildInstance(
          navMeta,
          row,
          includeTree[navName],
          true,
        );

        if (isArray) {
          addUnique(
            existing[navName],
            nestedObj,
            `${modelMeta.name}_${navName}`,
            navModelName,
          );
        } else {
          existing[navName] = nestedObj;
        }
      }
    }
  }

  return Object.values(itemsById);

  function isCidlModel(value: CidlType): value is { Model: string } {
    return typeof value === "object" && value !== null && "Model" in value;
  }

  function isCidlArray(value: CidlType): value is { Array: CidlType } {
    return typeof value === "object" && value !== null && "Array" in value;
  }

  function getCol(
    meta: MetaModel,
    attrName: string,
    row: Record<string, any>,
    prefixed: boolean,
  ) {
    return row[prefixed ? `${meta.name}_${attrName}` : attrName] ?? null;
  }

  function addUnique(arr: any[], item: any, key: string, navModelName: string) {
    seenNestedIds[key] = seenNestedIds[key] || new Set();

    // Get the primary key name for the nested model
    const navMeta = cidl.models[navModelName];
    const nestedPkAttr = navMeta?.primary_key;
    const nestedPkName = nestedPkAttr?.name || "id";

    const id = String(item[nestedPkName]);
    if (!seenNestedIds[key].has(id)) {
      arr.push(item);
      seenNestedIds[key].add(id);
    }
  }

  function buildInstance(
    meta: MetaModel,
    row: Record<string, any>,
    tree: Record<string, any>,
    prefixed: boolean,
  ): any {
    const instance = new constructorRegistry[meta.name]();

    // Assign PK
    instance[meta.primary_key.name] = getCol(
      meta,
      meta.primary_key.name,
      row,
      prefixed,
    );

    // Assign scalar attributes
    for (const attr of meta.attributes) {
      instance[attr.value.name] = getCol(meta, attr.value.name, row, prefixed);
    }

    // Assign navigation properties
    for (const nav of meta.navigation_properties) {
      const navName = nav.value.name;
      const navCidlType = nav.value.cidl_type;
      let navModelName: string | undefined;

      if (isCidlArray(navCidlType)) {
        if (isCidlModel(navCidlType.Array)) {
          navModelName = navCidlType.Array.Model;
        }
      } else if (isCidlModel(navCidlType)) {
        navModelName = navCidlType.Model;
      }

      if (!navModelName) continue;

      const navMeta = cidl.models[navModelName];
      if (!navMeta) continue;

      const nestedPk = navMeta.primary_key;
      const nestedId = row[`${navMeta.name}_${nestedPk.name}`];
      const isArray = isCidlArray(navCidlType);

      // Always initialize arrays, even if empty
      if (isArray) {
        instance[navName] = instance[navName] || [];
      }

      if (tree?.[navName] && nestedId != null) {
        const nestedObj = buildInstance(navMeta, row, tree[navName], true);
        if (isArray) {
          addUnique(
            instance[navName],
            nestedObj,
            `${meta.name}_${navName}`,
            navModelName,
          );
        } else {
          instance[navName] = nestedObj;
        }
      }
    }
    return instance;
  }
}

function error_state(status: number, message: string): HttpResult {
  return { ok: false, status, message };
}

function toResponse(r: HttpResult): Response {
  return new Response(JSON.stringify(r), {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}

interface MatchedRoute {
  modelMeta: MetaModel;
  methodMeta: ModelMethod;
  id: string | null;
}

/**
 * Each individual state of the `cloesce` function for testing purposes.
 */
export const _cloesceInternal = {
  matchRoute,
  validateRequest,
  hydrateModel,
  methodDispatch,
  _modelsFromSql,
};
