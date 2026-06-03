import { HttpResult } from "../ui/backend.js";
import { ApiMethod, Model } from "../cidl.js";
import { ApiImplementation } from "./router.js";

/**
 * Parses a CRUD method name into its (verb, dataSourceName) pair.
 * - `$verb` -> { verb, dataSourceName: "Default" }
 * - `$verb_DsName` -> { verb, dataSourceName: "DsName" }
 * - Anything else -> null
 */
function parseCrudName(
  name: string,
): { verb: "get" | "list" | "save"; dataSourceName: string } | null {
  if (!name.startsWith("$")) return null;
  const rest = name.slice(1);
  const underscoreIdx = rest.indexOf("_");
  const verb = (underscoreIdx === -1 ? rest : rest.slice(0, underscoreIdx)) as
    | "get"
    | "list"
    | "save";
  if (verb !== "get" && verb !== "list" && verb !== "save") return null;
  const dataSourceName = underscoreIdx === -1 ? "Default" : rest.slice(underscoreIdx + 1);
  return { verb, dataSourceName };
}

/**
 * Returns the implementation for a CRUD route by consulting the registered
 * model namespace (produced by `Model.impl({...})`). The generated code merges
 * its `GeneratedSource` defaults with any user overrides, so this lookup yields
 * a function whether the user implemented it or not.
 */
export function crudRoute(
  meta: Model,
  method: ApiMethod,
  userNamespace: any,
): ApiImplementation | null {
  const parsed = parseCrudName(method.name);
  if (!parsed) return null;

  const { verb, dataSourceName } = parsed;
  const dataSource = meta.data_sources[dataSourceName];
  if (!dataSource) return null;

  const dsNamespace = userNamespace?.[dataSourceName];
  const stub = dsNamespace?.[verb];
  if (typeof stub !== "function") {
    return () =>
      HttpResult.fail(
        501,
        `${meta.name}.${dataSourceName}.${verb} is declared in the schema but no implementation was provided.`,
      );
  }

  // Call the stub with `this` bound to the data source namespace so the generated
  // default impls can reference `this.tree` / `this.getQuery` / `this.listQuery`.
  return async (...args: unknown[]) => stub.apply(dsNamespace, args);
}

export function dataSourceStub(
  meta: Model,
  dsName: string,
  verb: "get" | "list" | "save",
  userNamespace: any,
): ((...args: unknown[]) => Promise<unknown>) | undefined {
  const ds = meta.data_sources[dsName];
  if (!ds) return undefined;
  const stub = userNamespace?.[dsName]?.[verb];
  return typeof stub === "function" ? stub : undefined;
}
