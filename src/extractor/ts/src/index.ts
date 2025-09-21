// Compiler hints
export const D1: ClassDecorator = () => {};
export const PrimaryKey: PropertyDecorator = () => {};
export const GET: MethodDecorator = () => {};
export const POST: MethodDecorator = () => {};
export const PUT: MethodDecorator = () => {};
export const PATCH: MethodDecorator = () => {};
export const DELETE: MethodDecorator = () => {};
export const DataSource: PropertyDecorator = () => {};
export const OneToMany =
  (_: string): PropertyDecorator =>
  () => {};
export const OneToOne =
  (_: string): PropertyDecorator =>
  () => {};

export const ForeignKey =
  <T>(_: T): PropertyDecorator =>
  () => {};

// API Result
export type Ok<T = void> = { ok: true; data: T };
export type Err = { ok: false; status: number; message: string };
export type Result<T = void> = Ok<T> | Err;

// Result factory
function ok(): Ok<void>;
function ok<T>(data: T): Ok<T>;
function ok<T>(data?: T): Ok<T | void> {
  return { ok: true, data: data as T | void };
}
function err(status: number, message: string): Err {
  return { ok: false, status, message };
}
export const Result = { ok, err };

// Include Tree
type Primitive = string | number | boolean | bigint | symbol | null | undefined;
export type IncludeTree<T> = T extends Primitive
  ? never
  : {
      [K in keyof T]?: T[K] extends (infer U)[]
        ? IncludeTree<NonNullable<U>>
        : IncludeTree<NonNullable<T[K]>>;
    };

/**
 * TODO: This could be WASM
 *
 * TODO: This is all GPT slop
 *
 * @returns JSON of type T
 */
export function mapSql<T>(rows: Record<string, any>[]): T[] {
  const result: any[] = [];
  const entityMaps: Record<string, Map<string, any>> = {};

  function getEntityKey(entityObj: any): string {
    // Use JSON string of all values as a "unique key" for deduplication
    return JSON.stringify(entityObj);
  }

  for (const row of rows) {
    const rowEntities: Record<string, any> = {};

    // Step 1: split row into entities by prefix
    for (const col in row) {
      const parts = col.split("_");
      if (parts.length < 2) continue;

      const entity = parts[0];
      const field = parts.slice(1).join("_");

      if (!rowEntities[entity]) rowEntities[entity] = {};
      rowEntities[entity][field] = row[col];
    }

    // Step 2: merge entities into result
    let topObj: any = null;

    for (const entity in rowEntities) {
      const entityObj = rowEntities[entity];
      const key = getEntityKey(entityObj);

      if (!entityMaps[entity]) entityMaps[entity] = new Map();
      if (!entityMaps[entity].has(key)) {
        entityMaps[entity].set(key, entityObj);
      }

      if (!topObj) {
        topObj = entityObj; // first entity becomes top-level
      } else {
        // If entity already exists on topObj
        if (!topObj[entity]) {
          topObj[entity] = [];
        }
        // Only push if not already in array
        if (!topObj[entity].some((o: any) => getEntityKey(o) === key)) {
          topObj[entity].push(entityObj);
        }
      }
    }

    if (topObj && !result.includes(topObj)) {
      result.push(topObj);
    }
  }

  return result as T[];
}

export function match(
  router: any,
  path: string,
  request: Request,
  env: any
): Response {
  const segments = path.split("/").filter(Boolean);
  const params: string[] = [];
  let node: any = router;

  const notFound = () =>
    new Response(JSON.stringify({ error: "Route not found", path }), {
      status: 404,
      headers: { "Content-Type": "application/json" },
    });

  for (const segment of segments) {
    if (node[segment]) {
      node = node[segment];
      continue;
    }

    const paramKey = Object.keys(node).find(
      (k) => k.startsWith("<") && k.endsWith(">")
    );
    if (!paramKey) return notFound();

    params.push(segment);
    node = node[paramKey];
  }

  return typeof node === "function"
    ? node(...params, request, env)
    : notFound();
}
