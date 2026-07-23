import { Cidl, Model, DataSource, IncludeTree } from "../cidl.js";
import { Orm } from "../router/orm.js";
import { RuntimeContainer } from "../router/router.js";
import { HttpResult } from "../ui/backend.js";
import { CloesceError, CloesceResult, InternalError } from "../common.js";

/**
 * @internal
 * The lazily-consulted registry mapping a model name to its registered
 * implementation module.
 */
export type Registry = Map<string, any>;

function storeKey(modelName: string): string {
  return modelName.length === 0 ? modelName : modelName[0].toLowerCase() + modelName.slice(1);
}

/**
 * Proxy traps that make an overlay object behave as an upgraded binding handle for a
 * *per-request child* env:
 * - overlay members (this env's model stores) win
 * - everything else is forwarded to the shared raw binding with `this` preserved.
 */
export function overlayTraps(fallback: any): ProxyHandler<any> {
  return {
    get(target, prop, receiver) {
      if (prop in target) {
        return Reflect.get(target, prop, receiver);
      }
      const value = fallback[prop];
      return typeof value === "function" ? value.bind(fallback) : value;
    },
    set(target, prop, value) {
      target[prop] = value;
      return true;
    },
    has(target, prop) {
      return prop in target || prop in fallback;
    },
  };
}

/**
 * @internal
 * Upgrade a raw Cloudflare binding in place with Cloesce's field helpers, reached under its
 * declared name (e.g. `env.SubRedditDb`).
 *
 * Binding is assigned in place so it stays a real `D1Database` / `KVNamespace` / ... that native
 * host APIs accept. Model stores are added later by {@link attachStores}.
 */
export function attachBinding(env: any, rawName: string, helpers: object = {}): void {
  const raw = env[rawName];
  if (raw == null) {
    return;
  }
  Object.assign(raw, helpers);
}

/** Return the handle for a binding on `env`. */
function ownBindingHandle(env: any, binding: string): any {
  if (Object.prototype.hasOwnProperty.call(env, binding)) {
    return env[binding];
  }
  const raw = env[binding];
  if (raw == null) {
    return undefined;
  }
  const handle = new Proxy({}, overlayTraps(raw));
  Object.defineProperty(env, binding, { value: handle, enumerable: true, configurable: true });
  return handle;
}

/** Map a `CloesceResult<T>` into an `HttpResult<T>`, 404 on a null value. */
function toHttp<T>(res: CloesceResult<T>): HttpResult<T> {
  if (res.errors.length > 0) {
    return HttpResult.fail(400, CloesceError.displayErrors(res)) as HttpResult<T>;
  }
  if (res.value === null || res.value === undefined) {
    return HttpResult.fail(404) as HttpResult<T>;
  }
  return HttpResult.ok(200, res.value);
}

/** Route/shard-key values pulled off a model value, for hydrating unseeded children. */
function routeParams(model: Model, value: any): Record<string, unknown> {
  const params: Record<string, unknown> = {};
  for (const field of model.route_fields) {
    if (value != null && value[field.name] !== undefined) params[field.name] = value[field.name];
  }
  return params;
}

/**
 * @internal
 * Build the store object for a single data source.
 *
 * Each verb first consults the registered impl (for an override or custom source)
 * and otherwise delegates to the generic ORM using the source's serialized include
 * tree and plan.
 */
function buildSourceVerbs(env: any, cidl: Cidl, model: Model, ds: DataSource, registry: Registry) {
  const meta = model as any;

  const userVerb = (verb: "get" | "list" | "save") => registry.get(model.name)?.[ds.name]?.[verb];

  const getNames = ds.get.parameters.map((p) => p.parameter.name);
  const listNames = ds.list.parameters.map((p) => p.name);
  const saveNames = ds.save.parameters.map((p) => p.name);

  const get = async (...args: unknown[]): Promise<HttpResult<any>> => {
    const override = userVerb("get");
    if (override) {
      return await coerceHttp(override(env, ...args));
    }

    await RuntimeContainer.init(cidl);

    const params = zip(getNames, args);
    const res = await Orm.fromEnv(env).get(meta, params, ds.tree, ds.get_plan as any);
    return toHttp(res);
  };

  const list = async (...args: unknown[]): Promise<HttpResult<any[]>> => {
    const override = userVerb("list");
    if (override) {
      return await coerceHttp(override(env, ...args));
    }

    await RuntimeContainer.init(cidl);

    const params = zip(listNames, args);
    const res = await Orm.fromEnv(env).list(meta, params, ds.tree, ds.list_plan as any);
    if (res.errors.length > 0) {
      return HttpResult.fail(400, CloesceError.displayErrors(res));
    }
    return HttpResult.ok(200, res.value!);
  };

  const save = async (...args: unknown[]): Promise<HttpResult<any>> => {
    const override = userVerb("save");
    if (override) {
      return await coerceHttp(override(env, ...args));
    }

    await RuntimeContainer.init(cidl);

    // Generated default sources always name the payload parameter "model"; the other
    // params are scalar shard/route keys merged onto it.
    const modelIdx = saveNames.indexOf("model");
    if (modelIdx === -1) {
      throw new InternalError(`Cloesce store.save for "${model.name}" has no "model" parameter.`);
    }

    const payload: any = { ...(args[modelIdx] as object) };
    saveNames.forEach((name, i) => {
      if (i !== modelIdx) {
        payload[name] = args[i];
      }
    });
    const res = await Orm.fromEnv(env).save(meta, payload, ds.tree);
    return toHttp(res);
  };

  const hydrate = async (row: any, ...rest: unknown[]): Promise<HttpResult<any>> => {
    await RuntimeContainer.init(cidl);

    const params = model.route_fields.length
      ? zip(
          model.route_fields.map((f) => f.name),
          rest,
        )
      : routeParams(model, row);

    const res = await Orm.fromEnv(env).hydrate(meta, row, ds.tree, ds.list_plan as any, params);
    return toHttp(res);
  };

  const hydrateAll = async (rows: any[], ...rest: unknown[]): Promise<HttpResult<any[]>> => {
    await RuntimeContainer.init(cidl);

    const params = model.route_fields.length
      ? zip(
          model.route_fields.map((f) => f.name),
          rest,
        )
      : {};

    const res = await Orm.fromEnv(env).hydrateAll(meta, rows, ds.tree, ds.list_plan as any, params);
    if (res.errors.length > 0) {
      return HttpResult.fail(400, CloesceError.displayErrors(res));
    }
    return HttpResult.ok(200, res.value!);
  };

  const load = async (self: any, includeTree: unknown): Promise<HttpResult<any>> => {
    await RuntimeContainer.init(cidl);
    const res = await Orm.fromEnv(env).hydrate(
      meta,
      self,
      includeTree as IncludeTree,
      undefined,
      routeParams(model, self),
    );

    return toHttp(res);
  };

  return { tree: ds.tree, get, list, save, hydrate, hydrateAll, load };
}

/**
 * @internal
 * Env stores receive all API routes for the model.
 */
function buildRouteWrappers(env: any, model: Model, registry: Registry) {
  const wrappers: Record<string, (...args: unknown[]) => Promise<HttpResult<any>>> = {};
  for (const api of model.apis) {
    if (api.name.startsWith("$")) {
      // Skip CRUD routes; they are handled by the source store.
      continue;
    }

    const hasEnv = api.injected.length > 0 || api.durable_target != null;
    const envIndex = api.is_static ? 0 : 1;
    wrappers[api.name] = async (...callArgs: unknown[]): Promise<HttpResult<any>> => {
      const impl = registry.get(model.name)?.[api.name];
      if (typeof impl !== "function") {
        return HttpResult.fail(
          501,
          `${model.name}.${api.name} is declared in the schema but no implementation was registered.`,
        );
      }
      const args = [...callArgs];
      if (hasEnv) {
        args.splice(envIndex, 0, env);
      }
      return coerceHttp(impl(...args));
    };
  }
  return wrappers;
}

/** Wrap a route/source return (bare value or HttpResult) into an HttpResult. */
async function coerceHttp(res: any): Promise<HttpResult<any>> {
  const awaited = await res;
  return awaited instanceof HttpResult ? awaited : HttpResult.ok(200, awaited);
}

function zip(names: string[], args: unknown[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  names.forEach((name, i) => (out[name] = args[i]));
  return out;
}

/** Where the router finds every model's store regardless of backing. */
const STORE_REGISTRY_KEY = "__cloesceStores";

/**
 * @internal
 * Build a store for every model that declares a data source.
 * - Each store is recorded in `env.__cloesceStores` so `self` hydration and CRUD dispatch can
 *   find it.
 * - Models backed by a D1/DO binding are exposed at the user-facing `env.<binding>.<model>`.
 * - Models with a data source but no binding (KV/R2-only, or route-only with API routes) are
 *   exposed directly at `env.<model>` instead.
 * - Verbs and routes are env-bound (they close over `env`), so callers never re-pass `env`.
 */
export function attachStores(env: any, cidl: Cidl, registry: Registry): void {
  const stores: Record<string, any> = {};
  Object.defineProperty(env, STORE_REGISTRY_KEY, {
    value: stores,
    enumerable: false,
    configurable: true,
  });

  for (const model of Object.values(cidl.models)) {
    if (Object.keys(model.data_sources).length === 0) {
      continue;
    }

    const store: any = {};
    for (const ds of Object.values(model.data_sources)) {
      const verbs = buildSourceVerbs(env, cidl, model, ds, registry);
      if (ds.name === "Default") {
        Object.assign(store, verbs);
      } else {
        store[storeKey(ds.name)] = verbs;
      }
    }

    for (const [name, wrapper] of Object.entries(buildRouteWrappers(env, model, registry))) {
      if (!(name in store)) {
        store[name] = wrapper;
      }
    }
    stores[model.name] = store;

    const binding = model.backing?.binding;
    if (binding) {
      const handle = ownBindingHandle(env, binding);
      if (handle != null) {
        handle[storeKey(model.name)] = store;
      }
    } else {
      env[model.name] = store;
    }
  }
}

/** @internal Resolve the store for a model's default source from the upgraded env. */
export function modelStore(env: any, model: Model): any {
  return env?.[STORE_REGISTRY_KEY]?.[model.name];
}

/** @internal Resolve a named-source store (or the default store) for a model. */
export function sourceStore(env: any, model: Model, sourceName: string): any {
  const store = modelStore(env, model);
  if (!store) {
    return undefined;
  }
  return sourceName === "Default" ? store : store[storeKey(sourceName)];
}
