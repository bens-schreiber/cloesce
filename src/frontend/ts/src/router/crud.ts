import { D1Database } from "@cloudflare/workers-types/experimental";
import {
  CrudKind,
  HttpResult,
  HttpVerb,
  Model,
  ModelMethod,
  NULL_DATA_SOURCE,
} from "../common";
import { Orm } from "../ui/backend";

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
   * Returns a CRUD Model Method if the method is a built-in CRUD method.
   */
  static getModelMethod(s: string, model: Model): ModelMethod | undefined {
    if (!model.cruds.includes(s as CrudKind)) {
      return undefined;
    }

    return {
      POST: {
        name: "POST",
        is_static: true,
        http_verb: HttpVerb.POST,
        return_type: { HttpResult: { Object: model.name } },
        parameters: [
          {
            name: "obj",
            cidl_type: { Partial: model.name },
          },
          {
            name: "dataSource",
            cidl_type: { Nullable: "Text" },
          },
        ],
      },
      PATCH: {
        name: "PATCH",
        is_static: false,
        http_verb: HttpVerb.PATCH,
        return_type: { HttpResult: { Object: model.name } },
        parameters: [
          {
            name: "obj",
            cidl_type: { Partial: model.name },
          },
          {
            name: "dataSource",
            cidl_type: { Nullable: "Text" },
          },
        ],
      },
      GET: {
        name: "GET",
        is_static: true,
        http_verb: HttpVerb.GET,
        return_type: { HttpResult: { Object: model.name } },
        parameters: [
          {
            name: "id",
            cidl_type: model.primary_key.cidl_type,
          },
          {
            name: "dataSource",
            cidl_type: { Nullable: "Text" },
          },
        ],
      },
      LIST: {
        name: "LIST",
        is_static: true,
        http_verb: HttpVerb.GET,
        return_type: { HttpResult: { Array: { Object: model.name } } },
        parameters: [
          {
            name: "dataSource",
            cidl_type: { Nullable: "Text" },
          },
        ],
      },
    }[s] as ModelMethod | undefined;
  }

  /**
   * Invokes a method on the instance, intercepting built-in CRUD methods and injecting
   * a default definition.
   */
  interceptCrud(methodName: string): Function {
    const map: Record<string, Function> = {
      POST: this.upsert.bind(this),
      PATCH: this.upsert.bind(this),
      GET: this.get.bind(this),
      LIST: this.list.bind(this),
    };

    const fn = this.instance && (this.instance as any)[methodName];
    return fn ? fn.bind(this.instance) : map[methodName];
  }

  async upsert(obj: object, dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = normalizeDs(dataSource);
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
    const normalizedDs = normalizeDs(dataSource);

    const orm = Orm.fromD1(this.d1);
    const res = await orm.get(this.ctor, id, normalizedDs as any);
    return res.ok
      ? { ok: true, status: 200, data: res.value }
      : { ok: false, status: 500, data: res.value };
  }

  async list(dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = normalizeDs(dataSource);

    const orm = Orm.fromD1(this.d1);
    const res = await orm.list(this.ctor, normalizedDs as any);
    return res.ok
      ? { ok: true, status: 200, data: res.value }
      : { ok: false, status: 500, data: res.value };
  }
}

function normalizeDs(ds: string): string | null {
  return ds === NULL_DATA_SOURCE ? null : ds;
}
