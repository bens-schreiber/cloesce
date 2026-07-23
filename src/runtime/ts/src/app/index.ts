import { Cidl } from "../cidl.js";
import { RuntimeContainer, router } from "../router/router.js";
import { attachStores, overlayTraps } from "./store.js";
import { durableSqlBatch } from "../router/orm.js";
import { applyDurableMigrations, DurableMigration } from "../ui/migrations.js";
import { DurableObjectState } from "@cloudflare/workers-types";

export { attachStores, attachBinding } from "./store.js";

/**
 * Attach the Cloesce RPC surface onto a Durable Object instance's prototype.
 */
export function attachDurableRpc(host: object): void {
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

/** Registration handle for a deployable host; `Models` is the host's owed set. */
export interface HostTag<Models extends string> {
  readonly __models?: Models;
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

/** Build a host registration handle. */
export function hostTag<Models extends string>(): HostTag<Models> {
  return {} as HostTag<Models>;
}

/** Build an injectable registration handle. */
export function injectableTag<Name extends string, T>(name: Name): InjectableTag<Name, T> {
  return { __name: name, __injectable: true } as InjectableTag<Name, T>;
}

/**
 * The typed assembly builder. Utilizes a phantom union to surface missing
 * bindings at compile time.
 *
 * - `register` supplies a model's implementation or an injectable's value widening the phantom `Reg` union.
 * - `run` is not callable until `Reg` covers the host's owed set `Owed`.
 * - `Env` is the host's fully-upgraded environment, exposed via `env`.
 */
export interface AppBuilder<Owed extends string, Reg extends string, Env> {
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
  ): AppBuilder<Owed, Reg | Name, Env>;

  run: [Owed] extends [Reg]
    ? (request: Request) => Promise<Response>
    : MissingBindings<{ unregistered: Exclude<Owed, Reg> }>;
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
 * Untyped runtime app. The generated `createApp` upgrades the binding template helpers,
 * then constructs one of these and casts it to the typed {@link AppBuilder}.
 */
export class RuntimeApp {
  readonly env: any;
  private registry = new Map<string, any>();

  /** Per-builder injectable values. Never written onto the shared `env` (see `register`). */
  private injected: Record<string, any> = {};

  constructor(
    private readonly cidl: Cidl,
    private readonly workerUrl: string,
    env: any,
    private readonly ctx?: DurableObjectState,
    migrations: DurableMigration[] = [],
  ) {
    this.env = env;
    attachStores(this.env, cidl, this.registry);
    if (ctx && migrations.length > 0) {
      ctx.blockConcurrencyWhile(() => applyDurableMigrations(ctx.storage as any, migrations));
    }
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
    return router(request, this.cidl, this.workerUrl, this.env, this.registry, this.ctx);
  }

  /** Force the ORM WASM module to initialize (for tests that read `env` before `run`). */
  async forceLoad(): Promise<void> {
    await RuntimeContainer.init(this.cidl);
  }
}

/**  Construct a runtime app. */
export function makeApp(
  cidl: Cidl,
  workerUrl: string,
  env: any,
  ctx?: DurableObjectState,
  migrations: DurableMigration[] = [],
): RuntimeApp {
  return new RuntimeApp(cidl, workerUrl, env, ctx, migrations);
}
