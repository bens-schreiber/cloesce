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

// Result
export type Result<T = void> = {
  ok: boolean;
  status: number;
  data?: T;
  message?: string;
};

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
 * TODO: I hit ChatGPT with a hammer 10-15 times until it spit out this algorithm
 * that seems to work. No clue what it does, some day I might look into it.
 *
 * @returns JSON of type T
 */

export function mapSql<T>(rows: Record<string, any>[]): T[] {
  const result: any[] = [];
  const entityMaps: Record<string, Map<string, any>> = {};

  for (const row of rows) {
    const topObj: Record<string, any> = {};
    const nestedObjs: Record<string, any> = {};

    for (const col in row) {
      const idx = col.lastIndexOf("_");
      if (idx === -1) {
        topObj[col] = row[col];
        continue;
      }

      const entity = col.slice(0, idx);
      const field = col.slice(idx + 1);

      if (!nestedObjs[entity]) nestedObjs[entity] = {};
      nestedObjs[entity][field] = row[col];
    }

    const entityKeys = Object.keys(nestedObjs);
    let topEntity: string | null = null;

    // Automatically detect top-level entity: the one with all non-null values
    for (const entity of entityKeys) {
      const obj = nestedObjs[entity];
      if (!Object.values(obj).every((v) => v === null)) {
        if (!topEntity) topEntity = entity; // first non-null entity is top
      }
    }

    for (const entity of entityKeys) {
      const obj = nestedObjs[entity];
      if (Object.values(obj).every((v) => v === null)) continue;

      if (!entityMaps[entity]) entityMaps[entity] = new Map();
      const idKey = obj.id ?? JSON.stringify(obj);
      if (!entityMaps[entity].has(idKey)) {
        entityMaps[entity].set(idKey, obj);
      }

      const entityObj = entityMaps[entity].get(idKey);

      if (entity === topEntity) {
        // assign directly to top-level object
        Object.assign(topObj, entityObj);
      } else {
        if (!topObj[entity]) topObj[entity] = [];
        topObj[entity].push(entityObj);
      }
    }

    result.push(topObj);
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
