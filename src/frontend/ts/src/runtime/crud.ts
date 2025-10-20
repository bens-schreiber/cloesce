import { D1Database } from "@cloudflare/workers-types/experimental";
import { HttpResult, HttpVerb, Model, ModelMethod } from "../common";
import { Orm } from "../index/backend";

export class CrudWrapper {
  public constructor(
    public d1: D1Database,
    public instance: any | undefined,
    public ctor: new () => object
  ) {}

  static getModelMethod(s: string, modelName: string): ModelMethod | undefined {
    return {
      POST: {
        name: "POST",
        is_static: true,
        http_verb: HttpVerb.POST,
        return_type: { HttpResult: { Object: modelName } },
        parameters: [
          {
            name: "obj",
            cidl_type: { Partial: modelName },
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
        return_type: { HttpResult: { Object: modelName } },
        parameters: [
          {
            name: "obj",
            cidl_type: { Partial: modelName },
          },
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

    return { ok: true, status: 201, data: getRes.value };
  }
}
