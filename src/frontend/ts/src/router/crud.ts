import { D1Database } from "@cloudflare/workers-types/experimental";
import { HttpResult, NO_DATA_SOURCE } from "../common.js";
import { Orm } from "../ui/backend.js";

/**
 * A wrapper for Model Instances, containing definitions for built-in CRUD methods.
 */
export class CrudContext {
  private constructor(
    private d1: D1Database,
    private instance: object | undefined,
    private ctor: new () => object,
  ) {}

  static fromInstance(
    d1: D1Database,
    instance: any,
    ctor: new () => object,
  ): CrudContext {
    return new this(d1, instance, ctor);
  }

  static fromCtor(d1: D1Database, ctor: new () => object): CrudContext {
    return new this(d1, ctor, ctor);
  }

  /**
   * Invokes a method on the instance, intercepting built-in CRUD methods and injecting
   * a default definition.
   */
  interceptCrud(methodName: string): Function {
    const map: Record<string, Function> = {
      post: this.upsert.bind(this),
      patch: this.upsert.bind(this),
      get: this.get.bind(this),
      list: this.list.bind(this),
    };

    const fn = this.instance && (this.instance as any)[methodName];
    return fn ? fn.bind(this.instance) : map[methodName];
  }

  async upsert(obj: object, dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = dataSource === NO_DATA_SOURCE ? null : dataSource;
    const includeTree = normalizedDs ? (this.ctor as any)[normalizedDs] : null;

    // Upsert
    const orm = Orm.fromD1(this.d1);
    const upsert = await orm.upsert(this.ctor, obj, includeTree);
    if (!upsert.ok) {
      return { ok: false, status: 500, data: upsert.value }; // TODO: better status code?
    }

    // Get
    const get = await orm.get(this.ctor, upsert.value, normalizedDs as any);
    return get.ok
      ? { ok: true, status: 200, data: get.value }
      : { ok: false, status: 500, data: get.value };
  }

  async get(id: any, dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = dataSource === NO_DATA_SOURCE ? null : dataSource;
    const orm = Orm.fromD1(this.d1);
    const res = await orm.get(this.ctor, id, normalizedDs as any);
    return res.ok
      ? { ok: true, status: 200, data: res.value }
      : { ok: false, status: 500, data: res.value };
  }

  async list(dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = dataSource === NO_DATA_SOURCE ? null : dataSource;
    const orm = Orm.fromD1(this.d1);
    const res = await orm.list(this.ctor, normalizedDs as any);
    return res.ok
      ? { ok: true, status: 200, data: res.value }
      : { ok: false, status: 500, data: res.value };
  }
}
