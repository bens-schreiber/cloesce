// @ts-nocheck

export interface Env {
  DB: D1Database;
}

type SQLRow = Record<string, any>;

/**
 * TODO: This could be WASM
 * TODO: This is all GPT slop
 *
 * @returns JSON of type T
 */
export function mapSql<T>(rows: SQLRow[]): T[] {
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

function match(
  router: any,
  path: string,
  request: Request,
  env: Env
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
