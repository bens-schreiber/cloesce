import * as clo from "./backend.js";
import { HttpResult } from "cloesce";

const NoLettuce = clo.Hamburger.NoLettuce.impl({
  async get(env, id) {
    const stmt = env.db
      .prepare(
        `WITH included AS (${this.selectQuery})
           SELECT * FROM included
           WHERE "toppings.topping.name" != 'LETTUCE'
           AND id = ?1
           ORDER BY id`,
      )
      .bind(id);
    const res = await clo.Hamburger.Orm.get(env, { query: stmt, include: this.tree });
    if (res.errors.length > 0) {
      return HttpResult.fail(400, JSON.stringify(res.errors));
    }
    if (res.value === null) {
      return HttpResult.fail(404);
    }
    return HttpResult.ok(200, res.value);
  },
});

const BurgersWithLettuceOrdered = clo.Hamburger.BurgersWithLettuceOrdered.impl({
  async list(env, lastId, limit) {
    const stmt = env.db
      .prepare(
        `WITH included AS (${this.selectQuery})
           SELECT * FROM included
           WHERE "toppings.topping.name" = 'LETTUCE'
           AND id > ?1
           ORDER BY id
           LIMIT ?2`,
      )
      .bind(lastId, limit);

    const res = await clo.Hamburger.Orm.list(env, { query: stmt, include: this.tree });
    if (res.errors.length > 0) {
      return HttpResult.fail(400, JSON.stringify(res.errors));
    }
    return HttpResult.ok(200, res.value!);
  },
});

const OnlyBacon = clo.Hamburger.OnlyBacon.impl({
  async get(env, id) {
    const stmt = env.db
      .prepare(
        `WITH included AS (${this.selectQuery})
           SELECT * FROM included
           WHERE "toppings.topping.name" = 'BACON'
           AND id = ?1
           ORDER BY id`,
      )
      .bind(id);
    const res = await clo.Hamburger.Orm.get(env, { query: stmt, include: this.tree });
    if (res.errors.length > 0) {
      return HttpResult.fail(400, JSON.stringify(res.errors));
    }
    if (res.value === null) {
      return HttpResult.fail(404);
    }
    return HttpResult.ok(200, res.value);
  },
});

const Hamburger = clo.Hamburger.impl({
  noLettuceToppings(self) {
    return self.toppings.map((t) => t.topping);
  },

  onlyBaconToppings(self) {
    return self.toppings.map((t) => t.topping);
  },

  BurgersWithLettuceOrdered,
  NoLettuce,
  OnlyBacon,
});

const Topping = clo.Topping.impl({});

const Default = clo.DefaultOverride.Default.impl({
  get() {
    return { id: Number.MAX_VALUE };
  },
  list() {
    return [{ id: Number.MAX_VALUE }];
  },
});

const DefaultOverride = clo.DefaultOverride.impl({
  Default,
});

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(Hamburger).register(Topping).register(DefaultOverride);
    return app.run(request);
  },
};
