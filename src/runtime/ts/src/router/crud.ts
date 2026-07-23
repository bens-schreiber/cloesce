import { HttpResult } from "../ui/backend.js";
import { ApiMethod, Model } from "../cidl.js";
import { ApiImplementation } from "./router.js";
import { sourceStore } from "../app/store.js";

/**
 * Returns the implementation for a CRUD route (`$verb` / `$verb_Source`) by resolving
 * the matching verb off the model's env-attached store.
 */
export function crudRoute(meta: Model, method: ApiMethod, env: any): ApiImplementation | null {
  const parsed = parseCrudName(method.name);
  if (!parsed) {
    return null;
  }

  const { verb, dataSourceName } = parsed;
  const store = sourceStore(env, meta, dataSourceName);
  const fn = store?.[verb];
  if (typeof fn !== "function") {
    return () =>
      HttpResult.fail(
        501,
        `${meta.name}.${dataSourceName}.${verb} is declared in the schema but no implementation was provided.`,
      );
  }
  return async (...args: unknown[]) => fn(...args);
}

/**
 * Parses a CRUD method name into its (verb, dataSourceName) pair.
 * - `$verb` -> { verb, dataSourceName: "Default" }
 * - `$verb_DsName` -> { verb, dataSourceName: "DsName" }
 * - default -> null
 */
function parseCrudName(
  name: string,
): { verb: "get" | "list" | "save"; dataSourceName: string } | null {
  if (!name.startsWith("$")) {
    return null;
  }
  const rest = name.slice(1);
  const underscoreIdx = rest.indexOf("_");
  const verb = (underscoreIdx === -1 ? rest : rest.slice(0, underscoreIdx)) as
    | "get"
    | "list"
    | "save";
  if (verb !== "get" && verb !== "list" && verb !== "save") {
    return null;
  }
  const dataSourceName = underscoreIdx === -1 ? "Default" : rest.slice(underscoreIdx + 1);
  return { verb, dataSourceName };
}
