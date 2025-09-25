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

// TODO: Look into this more. Is the best option keeping the CIDL in memory here?
export function modelsFromSql<T>(
  modelName: string,
  cidl: any,
  records: Record<string, any>[],
  includeTree: IncludeTree<T>
): T[] {
  if (!records.length) return [];

  if (!cidl || !Array.isArray(cidl.models)) {
    throw new Error("Invalid CIDL: 'models' array is missing");
  }

  // Find the root model in the CIDL
  const modelMeta = cidl.models.find((m: any) => m.name === modelName);
  if (!modelMeta) throw new Error(`Model ${modelName} not found in CIDL`);

  const pkAttr = modelMeta.attributes.find((a: any) => a.is_primary_key);
  if (!pkAttr) throw new Error(`Primary key not found for ${modelName}`);
  const pkName = pkAttr.value.name;

  const itemsById: Record<string, any> = {};

  for (const row of records) {
    const isPrefixed = Object.keys(row).some((k) =>
      k.startsWith(`${modelName}_`)
    );
    const rootId = String(isPrefixed ? row[`${modelName}_id`] : row[pkName]);

    // Build root model if not exists
    if (!itemsById[rootId]) {
      itemsById[rootId] = buildModelFromRow(
        modelMeta,
        row,
        includeTree,
        isPrefixed,
        cidl
      );
    } else if (isPrefixed) {
      // Merge additional navigation rows into existing model
      mergeNavigations(
        itemsById[rootId],
        modelMeta,
        row,
        includeTree,
        isPrefixed,
        cidl
      );
    }
  }

  return Object.values(itemsById) as T[];

  // --- helper to build a model from a row ---
  function buildModelFromRow(
    meta: any,
    row: Record<string, any>,
    tree: any,
    isPrefixed: boolean,
    cidl: any
  ): any {
    const obj: any = {};

    // Map attributes
    for (const attr of meta.attributes) {
      const col = isPrefixed
        ? `${meta.name}_${attr.value.name}`
        : attr.value.name;
      obj[attr.value.name] = row[col] ?? null;
    }

    // Initialize navigation properties
    for (const nav of meta.navigation_properties) {
      const navName = nav.value.name;
      const navTypeArray = nav.value.cidl_type.Array?.Model;
      const navTypeModel = nav.value.cidl_type.Model;

      const navModelName = navTypeArray || navTypeModel;
      if (!navModelName) continue;

      const navMeta = cidl.models.find((m: any) => m.name === navModelName);
      if (!navMeta) continue;

      const navPk = navMeta.attributes.find((a: any) => a.is_primary_key);
      const nestedId = row[`${navMeta.name}_${navPk.value.name}`];

      if (tree?.[navName]) {
        // Include property from tree
        if (nestedId != null) {
          const nestedObj = buildModelFromRow(
            navMeta,
            row,
            tree[navName],
            true,
            cidl
          );
          if (navTypeArray) {
            obj[navName] = obj[navName] || [];
            if (
              !obj[navName].some((x: any) => x[navPk.value.name] === nestedId)
            ) {
              obj[navName].push(nestedObj);
            }
          } else if (navTypeModel) {
            obj[navName] = nestedObj;
          }
        } else if (navTypeArray) {
          obj[navName] = []; // array included but empty
        }
      } else if (navTypeArray) {
        // Not in tree but array -> initialize empty
        obj[navName] = [];
      }
      // One-to-one navs not in tree are skipped
    }

    return obj;
  }

  // --- helper to merge additional navigation rows into an existing object ---
  function mergeNavigations(
    obj: any,
    meta: any,
    row: Record<string, any>,
    tree: any,
    isPrefixed: boolean,
    cidl: any
  ) {
    for (const nav of meta.navigation_properties) {
      const navName = nav.value.name;
      if (!tree?.[navName]) continue;

      const navTypeArray = nav.value.cidl_type.Array?.Model;
      const navTypeModel = nav.value.cidl_type.Model;
      const navModelName = navTypeArray || navTypeModel;
      if (!navModelName) continue;

      const navMeta = cidl.models.find((m: any) => m.name === navModelName);
      if (!navMeta) continue;

      const navPk = navMeta.attributes.find((a: any) => a.is_primary_key);
      const nestedId = row[`${navMeta.name}_${navPk.value.name}`];
      if (nestedId == null) continue;

      const nestedObj = buildModelFromRow(
        navMeta,
        row,
        tree[navName],
        true,
        cidl
      );

      if (navTypeArray) {
        obj[navName] = obj[navName] || [];
        if (!obj[navName].some((x: any) => x[navPk.value.name] === nestedId)) {
          obj[navName].push(nestedObj);
        }
      } else if (navTypeModel) {
        obj[navName] = nestedObj;
      }
    }
  }
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
