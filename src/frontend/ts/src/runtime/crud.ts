import { D1Database } from "@cloudflare/workers-types/experimental";
import { HttpResult, HttpVerb, Model, ModelMethod } from "../common";
import { Orm } from "../index/backend";

export class CrudWrapper {
  public constructor(
    public d1: D1Database,
    public instance: any
  ) {}

  static getModelMethod(s: string, modelName: string): ModelMethod | undefined {
    return {
      POST: {
        name: "POST",
        is_static: true,
        http_verb: HttpVerb.POST,
        return_type: { Object: modelName },
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
        return_type: { Object: modelName },
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

  async upsert(
    obj: object,
    dataSource: string | null
  ): Promise<HttpResult<unknown>> {
    const orm = Orm.fromD1(this.d1);
    const ds = dataSource ? this.instance[dataSource] : null;
    const upsertRes = await orm.upsert(this.instance, obj, ds);
    if (!upsertRes.ok) {
      return { ok: false, status: 500, data: upsertRes.value }; // TODO: better status code?
    }

    const getRes = await orm.get(
      this.instance,
      upsertRes.value,
      dataSource as any
    );
    if (!getRes.ok) {
      return { ok: false, status: 500, data: getRes.value };
    }

    return { ok: true, status: 201, data: getRes.value };
  }
}
