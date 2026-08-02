import { Cidl, ENV_DURABLE_TARGET_KEY } from "../cidl.js";
import { RuntimeContainer, router } from "../router/router.js";
import { attachStores, overlayTraps } from "./store.js";
import { durableSqlBatch } from "../router/orm.js";
import { applyDurableMigrations, DurableMigration } from "../ui/migrations.js";
import { DurableObjectState } from "@cloudflare/workers-types";

export { attachStores, attachBinding } from "./store.js";

/**
 * Attach the Cloesce RPC surface onto a Durable Object instance's prototype.
 */
function attachDurableRpc(host: object): void {
  const proto: any = Object.getPrototypeOf(host);
  if (proto.__cloesceSqlBatch) {
    return;
  }

  proto.__cloesceSqlBatch = function (
    this: { ctx: DurableObjectState },
    statements: { sql: string; bindings: unknown[] }[],
  ) {
    return durableSqlBatch(this.ctx.storage as any, statements);
  };

  proto.__cloesceKvGet = function (this: { ctx: DurableObjectState }, key: string) {
    return this.ctx.storage.kv.get(key) ?? null;
  };

  proto.__cloesceKvGetMany = function (this: { ctx: DurableObjectState }, keys: string[]) {
    return keys.map((key) => [key, this.ctx.storage.kv.get(key) ?? null]);
  };

  proto.__cloesceKvPut = function (this: { ctx: DurableObjectState }, key: string, value: unknown) {
    this.ctx.storage.kv.put(key, value);
  };
}

/** Brand returned by the capability gate when a required binding is absent. */
export interface MissingBindings<M> {
  readonly __missing: M;
}

/**
 * The store member type `F`, but only if **every** required binding key is present on
 * the ambient env `E`. Otherwise a non-callable brand naming the missing bindings, so a
 * call fails to typecheck.
 */
export type Needs<E, Req extends string, F> = [Req] extends [keyof E]
  ? F
  : MissingBindings<Exclude<Req, keyof E>>;

/** A value or a promise of it — the return latitude routes are allowed. */
export type Awaitable<T> = T | Promise<T>;

/** Registration handle for a model; `Impl` is the model's full `Api.Of` contract. */
export interface ModelTag<Name extends string, Impl> {
  readonly __name: Name;
  readonly __impl?: Impl;
}

/** Registration handle for an injectable; `T` is the (augmentable) provided shape. */
export interface InjectableTag<Name extends string, T> {
  readonly __name: Name;
  readonly __type?: T;
  readonly __injectable: true;
}

/** Build a model registration handle. */
export function modelTag<Name extends string, Impl>(name: Name): ModelTag<Name, Impl> {
  return { __name: name } as ModelTag<Name, Impl>;
}

/** Build an injectable registration handle. */
export function injectableTag<Name extends string, T>(name: Name): InjectableTag<Name, T> {
  return { __name: name, __injectable: true } as InjectableTag<Name, T>;
}

export interface MissingSource {
  readonly __missingSource: true;
}
export interface SourceAlreadyBound {
  readonly __sourceAlreadyBound: true;
}

/**
 * The typed assembly builder. Utilizes phantom unions to surface missing
 * bindings/sources at compile time.
 *
 * - `worker`/`durable` bind the deployable's source (a raw `RawEnv`, or a Durable Object
 *   instance of type `DoInstance`)
 * - `register` supplies a model's implementation or an injectable's value widening the phantom `Reg` union.
 * - `run` is not callable until a source is bound and `Reg` covers the host's owed set `Owed`.
 * - `Env` is the host's fully-upgraded environment, exposed via `env`.
 */
export interface AppBuilder<
  Owed extends string,
  Reg extends string,
  Env,
  RawEnv,
  DoInstance,
  Bound extends boolean = false,
> {
  readonly env: Env;

  /**
   * Force the ORM WASM module to initialize before `run`.
   *
   * This is useful for tests and other in-process callers that read `env`
   * without first serving an HTTP request.
   */
  forceLoad(): Promise<void>;

  /**
   * Supply one thing the host owes:
   * - a model's implementation (`register(Model, impl)`)
   * - an injectable's value (`register(Injectable, value)`).
   */
  register<Name extends Owed, T>(
    tag: ModelTag<Name, T> | InjectableTag<Name, T>,
    value: NoInfer<T>,
  ): AppBuilder<Owed, Reg | Name, Env, RawEnv, DoInstance, Bound>;

  /** Bind a Worker's raw environment as this app's source. */
  worker: Bound extends true
    ? SourceAlreadyBound
    : (env: RawEnv) => AppBuilder<Owed, Reg, Env, RawEnv, DoInstance, true>;

  /** Bind a Durable Object instance (and optional migrations) as this app's source. */
  durable: Bound extends true
    ? SourceAlreadyBound
    : (
        durableObject: DoInstance,
        migrations?: DurableMigration[],
      ) => AppBuilder<Owed, Reg, Env, RawEnv, DoInstance, true>;

  run: Bound extends true
    ? [Owed] extends [Reg]
      ? (request: Request) => Promise<Response>
      : MissingBindings<{ unregistered: Exclude<Owed, Reg> }>
    : MissingSource;
}

function nameOf(tag: any): string {
  return typeof tag === "string" ? tag : tag.__name;
}

/**
 * A child env layered over `parent`: own writes (injectable values, per-child stores) collect on a
 * fresh overlay target, while reads fall through to `parent` for anything the overlay hasn't set.
 * Unlike `Object.create(parent)`, this never forwards writes into the shared Cloudflare host env,
 * keeping per-request injectables isolated.
 */
function overlayEnv(parent: any): any {
  return new Proxy({}, overlayTraps(parent));
}

/**
 * Untyped runtime app. The generated `createApp` constructs one of these (with no source
 * bound yet) and casts it to the typed {@link AppBuilder}. `worker`/`durable` bind the
 * source, upgrading bindings and attaching stores; `run` is only meaningful afterward.
 */
export class RuntimeApp {
  env: any;
  private registry = new Map<string, any>();

  /** Per-builder injectable values. Never written onto the shared `env` (see `register`). */
  private injected: Record<string, any> = {};

  constructor(
    private readonly cidl: Cidl,
    private readonly workerUrl: string,
    private readonly upgradeBindings: (env: any) => void,
  ) {}

  /** Bind a Worker's raw environment as this app's source. */
  worker(env: any): RuntimeApp {
    this.upgradeBindings(env);
    this.env = env;
    attachStores(this.env, this.cidl, this.registry);
    return this;
  }

  /** Bind a Durable Object instance (and optional migrations) as this app's source. */
  durable(durableObject: any, migrations: DurableMigration[] = []): RuntimeApp {
    attachDurableRpc(durableObject);
    const ctx: DurableObjectState = durableObject.ctx;
    const env = overlayEnv(durableObject.env);
    this.upgradeBindings(env);
    this.env = env;
    env[ENV_DURABLE_TARGET_KEY] = ctx;
    attachStores(this.env, this.cidl, this.registry);
    if (migrations.length > 0) {
      ctx.blockConcurrencyWhile(() => applyDurableMigrations(ctx.storage as any, migrations));
    }
    return this;
  }

  /**
   * Supply a model implementation or an injectable value.
   */
  register(tag: any, value: any): RuntimeApp {
    if (tag && tag.__injectable) {
      // Build a per-request child env that owns the injectable and its own env-bound stores
      // layered over the shared env. Writes must land on the overlay, never the shared env, so
      // overlapping requests that each register an injectable after an `await` stay isolated.
      const childEnv = overlayEnv(this.env);
      childEnv[nameOf(tag)] = value;
      attachStores(childEnv, this.cidl, this.registry);
      const next: RuntimeApp = Object.create(RuntimeApp.prototype);
      Object.assign(next, this, {
        env: childEnv,
        injected: { ...this.injected, [nameOf(tag)]: value },
      });
      return next;
    }
    this.registry.set(nameOf(tag), value);
    return this;
  }

  run(request: Request): Promise<Response> {
    return router(request, this.cidl, this.workerUrl, this.env, this.registry);
  }

  /** Force the ORM WASM module to initialize (for tests that read `env` before `run`). */
  async forceLoad(): Promise<void> {
    await RuntimeContainer.init(this.cidl);
  }
}

/**  Construct a runtime app with no source bound yet. */
export function makeApp(
  cidl: Cidl,
  workerUrl: string,
  upgradeBindings: (env: any) => void,
): RuntimeApp {
  return new RuntimeApp(cidl, workerUrl, upgradeBindings);
}
