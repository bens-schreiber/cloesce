import { D1Database } from "@cloudflare/workers-types/experimental";
import { CrudKind, HttpResult, HttpVerb, Model, ModelMethod } from "../common";
import { Orm } from "../index/backend";

export class CrudWrapper {
  public constructor(
    public d1: D1Database,
    public instance: any | undefined,
    public ctor: new () => object
  ) {}

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

  interceptCrud(methodName: string): Function {
    const map: Record<string, Function> = {
      POST: this.upsert.bind(this),
      PATCH: this.upsert.bind(this),
      GET: this.get.bind(this),
      LIST: this.list.bind(this),
    };

    const fn = this.instance[methodName];
    if (fn) {
      return fn.bind(this.instance);
    }

    return map[methodName];
  }

  async upsert(obj: object, dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = dataSource === "null" ? null : dataSource;
    const includeTree = normalizedDs ? (this.ctor as any)[normalizedDs] : null;

    // Upsert
    const orm = Orm.fromD1(this.d1);
    const upsertRes = await orm.upsert(this.ctor, obj, includeTree);
    if (!upsertRes.ok) {
      return { ok: false, status: 500, data: upsertRes.value }; // TODO: better status code?
    }

    // Get
    const getRes = await orm.get(
      this.ctor,
      upsertRes.value,
      normalizedDs as any
    );
    if (!getRes.ok) {
      return { ok: false, status: 500, data: getRes.value };
    }

    return { ok: true, status: 200, data: getRes.value };
  }

  async get(id: any, dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = dataSource === "null" ? null : dataSource;

    const orm = Orm.fromD1(this.d1);
    const getRes = await orm.get(this.ctor, id, normalizedDs as any);
    if (!getRes.ok) {
      return { ok: false, status: 500, data: getRes.value };
    }

    return { ok: true, status: 200, data: getRes.value };
  }

  async list(dataSource: string): Promise<HttpResult<unknown>> {
    const normalizedDs = dataSource === "null" ? null : dataSource;

    const orm = Orm.fromD1(this.d1);
    const list = await orm.list(this.ctor, normalizedDs as any);
    if (!list.ok) {
      return { ok: false, status: 500, data: list.value };
    }

    return { ok: true, status: 200, data: list.value };
  }
}
