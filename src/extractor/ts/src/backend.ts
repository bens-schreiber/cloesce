import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
import { IncludeTree, Result } from "./index.js";

// Either monad
type Either<L, R> = { ok: false; value: L } | { ok: true; value: R };
function left<L, R>(value: L): Either<L, R> {
  return { ok: false, value };
}
function right<L, R>(value: R): Either<L, R> {
  return { ok: true, value };
}

function error_state(status: number, message: string): Result {
  return { ok: false, status, message };
}

type CidlType =
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | "D1Database"
  | { Model: string }
  | { Array: CidlType }
  | { HttpResult: CidlType | null };

type HttpVerb = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

interface NamedTypedValue {
  name: string;
  cidl_type: CidlType;
  nullable: boolean;
}

interface ModelAttribute {
  value: NamedTypedValue;
  is_primary_key: boolean;
  foreign_key_reference: string | null;
}

interface ModelMethod {
  name: string;
  is_static: boolean;
  http_verb: HttpVerb;
  return_type: CidlType | null;
  parameters: NamedTypedValue[];
}

type NavigationPropertyKind =
  | { kind: "OneToOne"; reference: string }
  | { kind: "OneToMany"; reference: string }
  | { kind: "ManyToMany"; unique_id: string };

interface NavigationProperty {
  value: NamedTypedValue;
  kind: NavigationPropertyKind;
}

type CidlIncludeTree = Array<[NamedTypedValue, CidlIncludeTree]>;
interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

interface Model {
  name: string;
  attributes: ModelAttribute[];
  navigation_properties: NavigationProperty[];
  data_sources: DataSource[];
  methods: Record<string, ModelMethod>;
}

type Cidl = {
  models: Record<string, Model>;
  [key: string]: unknown;
};

type Constructor = new () => any;

export async function cloesce(
  rawCidl: any,
  constructorRegistry: Record<string, Constructor>,
  request: Request,
  api_route: string,
  d1: D1Database
): Promise<Response> {
  const toResponse = (r: Result) =>
    new Response(JSON.stringify(r), {
      status: r.status,
      headers: { "Content-Type": "application/json" },
    });

  // Interpet CIDL
  const cidl = {
    ...rawCidl,
    models: Object.fromEntries(
      rawCidl.models.map((m: any) => {
        const methodsMap: Record<string, any> = {};
        for (const method of m.methods) {
          methodsMap[method.name] = method;
        }

        return [m.name, { ...m, methods: methodsMap }];
      })
    ),
  };

  // 1. Route the HTTP request
  let route = matchRoute(request, api_route, cidl);
  if (!route.ok) {
    return toResponse(route.value);
  }

  // 2. Validate HTTP verb
  let { modelMeta, methodMeta, id } = route.value;
  let isValidHttp = validateHttp(request, methodMeta);
  if (!isValidHttp.ok) {
    return toResponse(isValidHttp.value);
  }

  // 3. Validate Request
  let isValidRequest = await validateRequest(request, cidl, methodMeta, id);
  if (!isValidRequest.ok) {
    return toResponse(isValidRequest.value);
  }
  let requestParams = isValidRequest.value;

  // 4. Data Hydration
  let instance: any;
  if (!methodMeta.is_static) {
    let successfulModel = await hydrateModel(
      cidl,
      modelMeta,
      constructorRegistry,
      d1,
      id!
    );

    if (!successfulModel.ok) {
      return toResponse(successfulModel.value);
    }

    instance = successfulModel.value;
  } else {
    instance = constructorRegistry[modelMeta.name];
  }

  // 5. Method Dispatch
  return toResponse(
    await methodDispatch(instance, methodMeta, requestParams, d1)
  );
}

interface Route {
  modelMeta: Model;
  methodMeta: ModelMethod;
  id: string | null;
}

// TODO: In the previous version, we would walk a generated trie
// This is more hardcode-y, and I'm not sure it will hold up to time.
function matchRoute(
  request: Request,
  api_route: string,
  cidl: Cidl
): Either<Result, Route> {
  const url = new URL(request.url);

  const err = left<Result, Route>(
    error_state(404, `Path not found ${url.pathname}`)
  );

  if (!url.pathname.startsWith(api_route)) return err;

  const routeParts = url.pathname
    .slice(api_route.length)
    .split("/")
    .filter(Boolean);

  if (routeParts.length < 2) return err;

  const modelName = routeParts[0];
  const methodName = routeParts[routeParts.length - 1];
  const id = routeParts.length === 3 ? routeParts[1] : null;

  const modelMeta = cidl.models[modelName];
  if (!modelMeta) return err;

  const methodMeta = modelMeta.methods[methodName];
  if (!methodMeta) return err;

  return right<Result, Route>({
    modelMeta,
    methodMeta,
    id,
  });
}

function validateHttp(
  request: Request,
  methodMeta: ModelMethod
): Either<Result, null> {
  const url = new URL(request.url);
  if (request.method !== methodMeta.http_verb) {
    return left<Result, null>(
      error_state(404, `Path not found ${url.pathname}`)
    );
  }

  return right<Result, null>(null);
}

async function validateRequest(
  request: Request,
  cidl: Cidl,
  methodMeta: ModelMethod,
  id: string | null
): Promise<Either<Result, any>> {
  if (methodMeta.parameters.length < 1) {
    return right<Result, any>({});
  }

  // Error state: any missing parameter, body, or malformed input will exit with 400.
  let invalid_request = left<Result, null>(
    error_state(400, "Invalid Request Body")
  );

  // Id's are required for instantaited methods.
  if (!methodMeta.is_static && id == null) {
    return invalid_request;
  }

  // D1Database is injected
  let requiredParams = methodMeta.parameters.filter(
    (p) => p.cidl_type !== "D1Database"
  );

  // Ensure that all parameters exist
  const url = new URL(request.url);
  let requestBody: any = undefined;
  if (methodMeta.http_verb === "GET") {
    let urlParams = url.searchParams;
    if (!requiredParams.every((p) => urlParams.has(p.name))) {
      return invalid_request;
    }
    requestBody = Object.fromEntries(url.searchParams.entries());
  } else {
    let body = await request.json();
    if (!requiredParams.every((p) => body?.[p.name] !== undefined)) {
      return invalid_request;
    }
    requestBody = body;
  }

  // Validate all parameters type
  for (const p of requiredParams) {
    const value = requestBody[p.name];
    if (!validateCidlType(value, p.cidl_type, cidl)) {
      console.log("INVALID CIDL TYPE!!!");
      return invalid_request;
    }
  }

  return right<Result, null>(requestBody);
}

function validateCidlType(value: any, cidlType: CidlType, cidl: Cidl): boolean {
  if (value === null || value === undefined) return false;

  // Handle primitive string types with switch
  if (typeof cidlType === "string") {
    console.log("IT'S A ASTRINGGGG", value, cidlType);
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

    // Validate attributes
    if (
      !model.attributes.every((attr) =>
        validateCidlType(value[attr.value.name], attr.value.cidl_type, cidl)
      )
    ) {
      return false;
    }

    // Validate navigation properties (optional)
    return model.navigation_properties.every((nav) => {
      const navValue = value[nav.value.name];
      return (
        navValue == null ||
        validateCidlType(navValue, nav.value.cidl_type, cidl)
      );
    });
  }

  if ("Array" in cidlType) {
    return (
      Array.isArray(value) &&
      value.every((v) => validateCidlType(v, cidlType.Array, cidl))
    );
  }

  if ("HttpResult" in cidlType) {
    if (value === null) return cidlType.HttpResult === null;
    if (cidlType.HttpResult === null) return false;
    return validateCidlType(value, cidlType.HttpResult, cidl);
  }

  return false;
}

async function hydrateModel(
  cidl: Cidl,
  modelMeta: Model,
  constructorRegistry: Record<string, Constructor>,
  d1: D1Database,
  id: string
): Promise<Either<Result, any>> {
  // Error state: If the D1 database has been tweaked outside of Cloesce
  // resulting in a malformed query, exit with a 500.
  const malformedQuery = (e: any) =>
    left<Result, any>(
      error_state(500, `${e instanceof Error ? e.message : String(e)}`)
    );

  // Error state: If no record is found for the id, return a 404
  const missingRecord = left<Result, any>(error_state(404, "Record not found"));

  // TODO: We are assuming defalt DS for now
  const pk = modelMeta.attributes.find((a) => a.is_primary_key)!;
  const hasDataSources = modelMeta.data_sources.length > 0;
  const query = hasDataSources
    ? `SELECT * FROM ${modelMeta.name}_default WHERE ${modelMeta.name}_${pk.value.name} = ?`
    : `SELECT * FROM ${modelMeta.name} WHERE ${pk.value.name} = ?`;

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

  // Convert the record to an instance of the Model
  let instance: any;
  if (hasDataSources) {
    // TODO: assuming default DS again
    let includeTree: any = new constructorRegistry[modelMeta.name]().default;
    let models: any[] = modelsFromSql(
      modelMeta.name,
      cidl,
      constructorRegistry,
      records.results,
      includeTree
    );
    instance = models[0];
  } else {
    instance = Object.assign(
      new constructorRegistry[modelMeta.name](),
      records.results[0]
    );
  }

  return right<Result, any>(instance);
}

async function methodDispatch(
  instance: any,
  methodMeta: ModelMethod,
  params: any,
  d1: D1Database
): Promise<Result<any>> {
  // Error state: Client code ran into an uncaught exception.
  const uncaughtException = (e: any) =>
    error_state(500, `${e instanceof Error ? e.message : String(e)}`);

  const paramArray = methodMeta.parameters.map((p) =>
    params[p.name] == undefined ? d1 : params[p.name]
  );

  try {
    return await instance[methodMeta.name](...paramArray);
  } catch (e) {
    return uncaughtException(e);
  }
}

export function modelsFromSql(
  modelName: string,
  cidl: Cidl,
  constructorRegistry: Record<string, Constructor>,
  records: Record<string, any>[],
  includeTree: Record<string, any>
): any[] {
  if (!records.length) return [];

  const modelMeta = cidl.models[modelName];
  if (!modelMeta) throw new Error(`Model ${modelName} not found in CIDL`);

  const pkAttr = modelMeta.attributes.find((a) => a.is_primary_key);
  if (!pkAttr) throw new Error(`Primary key not found for ${modelName}`);
  const pkName = pkAttr.value.name;

  const itemsById: Record<string, any> = {};
  const seenNestedIds: Record<string, Set<string>> = {};

  function isCidlModel(value: CidlType): value is { Model: string } {
    return typeof value === "object" && value !== null && "Model" in value;
  }

  function isCidlArray(value: CidlType): value is { Array: CidlType } {
    return typeof value === "object" && value !== null && "Array" in value;
  }

  const getCol = (
    attrName: string,
    row: Record<string, any>,
    prefixed: boolean
  ) => row[prefixed ? `${modelMeta.name}_${attrName}` : attrName] ?? null;

  const addUnique = (arr: any[], item: any, key: string) => {
    seenNestedIds[key] = seenNestedIds[key] || new Set();
    const id = String(item[Object.keys(item)[0]]);
    if (!seenNestedIds[key].has(id)) {
      arr.push(item);
      seenNestedIds[key].add(id);
    }
  };

  const buildInstance = (
    meta: Model,
    row: Record<string, any>,
    tree: Record<string, any>,
    prefixed: boolean
  ): any => {
    const instance = new constructorRegistry[meta.name]();

    // Assign scalar attributes
    for (const attr of meta.attributes) {
      instance[attr.value.name] = getCol(attr.value.name, row, prefixed);
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

      const nestedPkAttr = navMeta.attributes.find((a) => a.is_primary_key)!;
      const nestedId = row[`${navMeta.name}_${nestedPkAttr.value.name}`];

      const isArray = isCidlArray(navCidlType);
      if (isArray) instance[navName] = instance[navName] || [];

      if (tree?.[navName] && nestedId != null) {
        const nestedObj = buildInstance(navMeta, row, tree[navName], true);
        if (isArray)
          addUnique(instance[navName], nestedObj, `${meta.name}_${navName}`);
        else instance[navName] = nestedObj;
      } else if (isArray) {
        instance[navName] = instance[navName] || [];
      }
    }

    return instance;
  };

  for (const row of records) {
    const isPrefixed = Object.keys(row).some((k) =>
      k.startsWith(`${modelName}_`)
    );
    const rootId = String(isPrefixed ? row[`${modelName}_id`] : row[pkName]);

    const instance = buildInstance(modelMeta, row, includeTree, isPrefixed);

    if (!itemsById[rootId]) {
      itemsById[rootId] = instance;
      continue;
    }

    // Merge scalars and arrays for duplicates
    const existing = itemsById[rootId];
    for (const key in instance) {
      const val = instance[key];
      if (Array.isArray(val)) {
        existing[key] = existing[key] || [];
        val.forEach((item) =>
          addUnique(existing[key], item, `${modelMeta.name}_${key}`)
        );
      } else if (val != null) {
        existing[key] = val;
      }
    }
  }

  return Object.values(itemsById);
}
