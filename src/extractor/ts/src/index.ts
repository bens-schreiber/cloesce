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

  const modelMeta = cidl.models.find((m: any) => m.name === modelName);
  if (!modelMeta) throw new Error(`Model ${modelName} not found in CIDL`);

  const pkAttr = modelMeta.attributes.find((a: any) => a.is_primary_key);
  if (!pkAttr) throw new Error(`Primary key not found for ${modelName}`);
  const pkName = pkAttr.value.name;

  const itemsById: Record<string, any> = {};
  const seenNestedIds: Record<string, Set<string>> = {};

  const getCol = (
    meta: any,
    attrName: string,
    row: Record<string, any>,
    prefixed: boolean
  ) => row[prefixed ? `${meta.name}_${attrName}` : attrName] ?? null;

  const addUnique = (arr: any[], item: any, key: string) => {
    seenNestedIds[key] = seenNestedIds[key] || new Set();
    const id = String(item[Object.keys(item)[0]]);
    if (!seenNestedIds[key].has(id)) {
      arr.push(item);
      seenNestedIds[key].add(id);
    }
  };

  const buildOrMerge = (
    meta: any,
    row: Record<string, any>,
    tree: any,
    prefixed: boolean
  ) => {
    const model: any = {};

    // Add all attributes
    for (const attr of meta.attributes) {
      model[attr.value.name] = getCol(meta, attr.value.name, row, prefixed);
    }

    // Add navigation properties
    for (const nav of meta.navigation_properties) {
      const navName = nav.value.name;
      const navModelName =
        nav.value.cidl_type.Array?.Model || nav.value.cidl_type.Model;
      if (!navModelName) continue;

      const navMeta = cidl.models.find((m: any) => m.name === navModelName);
      if (!navMeta) continue;

      const navPkAttr = navMeta.attributes.find((a: any) => a.is_primary_key);
      const nestedId = row[`${navMeta.name}_${navPkAttr.value.name}`];

      const isArray = !!nav.value.cidl_type.Array?.Model;
      if (isArray) model[navName] = model[navName] || [];

      if (tree?.[navName] && nestedId != null) {
        const nestedObj = buildOrMerge(navMeta, row, tree[navName], true);
        if (isArray)
          addUnique(model[navName], nestedObj, `${model}_${navName}`);
        else model[navName] = nestedObj;
      } else if (isArray) {
        model[navName] = model[navName] || [];
      }
    }

    return model;
  };

  for (const row of records) {
    const isPrefixed = Object.keys(row).some((k) =>
      k.startsWith(`${modelName}_`)
    );
    const rootId = String(isPrefixed ? row[`${modelName}_id`] : row[pkName]);

    const merged = buildOrMerge(modelMeta, row, includeTree, isPrefixed);

    if (!itemsById[rootId]) {
      itemsById[rootId] = merged;
      continue;
    }

    // Merge scalars and arrays
    for (const key in merged) {
      const val = merged[key];
      if (Array.isArray(val)) {
        itemsById[rootId][key] = itemsById[rootId][key] || [];
        val.forEach((item) =>
          addUnique(itemsById[rootId][key], item, `${itemsById[rootId]}_${key}`)
        );
      } else if (val != null) {
        itemsById[rootId][key] = val;
      }
    }
  }

  return Object.values(itemsById) as T[];
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
