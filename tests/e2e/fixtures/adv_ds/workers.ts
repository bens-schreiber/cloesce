import * as clo from "./backend.js";
import { HttpResult } from "cloesce";

const NoLettuce = clo.Hamburger.NoLettuce.impl({
  async get(env, id) {
    const burger = await env.db
      .prepare(`SELECT "id", "name" FROM "Hamburger" WHERE "id" = ?1`)
      .bind(id)
      .first();
    if (!burger) {
      return HttpResult.fail(404);
    }
    const toppings = (
      await env.db
        .prepare(
          `SELECT ht."hamburgerId", ht."toppingId"
           FROM "HamburgerTopping" ht
           JOIN "Topping" t ON t."id" = ht."toppingId"
           WHERE ht."hamburgerId" = ?1 AND t."name" != 'LETTUCE'`,
        )
        .bind(id)
        .all()
    ).results;
    return await this.hydrate(env, { ...burger, toppings } as never);
  },
});

const OnlyBacon = clo.Hamburger.OnlyBacon.impl({
  async get(env, id) {
    const burger = await env.db
      .prepare(`SELECT "id", "name" FROM "Hamburger" WHERE "id" = ?1`)
      .bind(id)
      .first();
    if (!burger) {
      return HttpResult.fail(404);
    }
    const toppings = (
      await env.db
        .prepare(
          `SELECT ht."hamburgerId", ht."toppingId"
           FROM "HamburgerTopping" ht
           JOIN "Topping" t ON t."id" = ht."toppingId"
           WHERE ht."hamburgerId" = ?1 AND t."name" = 'BACON'`,
        )
        .bind(id)
        .all()
    ).results;
    return await this.hydrate(env, { ...burger, toppings });
  },
});

const BurgersWithLettuceOrdered = clo.Hamburger.BurgersWithLettuceOrdered.impl({
  async list(env, lastId, limit) {
    const rows = (
      await env.db
        .prepare(
          `SELECT h."id", h."name"
           FROM "Hamburger" h
           JOIN "HamburgerTopping" ht ON ht."hamburgerId" = h."id"
           JOIN "Topping" t ON t."id" = ht."toppingId"
           WHERE t."name" = 'LETTUCE' AND h."id" > ?1
           ORDER BY h."id" ASC LIMIT ?2`,
        )
        .bind(lastId, limit)
        .all()
    ).results;
    return await this.hydrateAll(env, rows);
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
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(Hamburger, Topping, DefaultOverride);
    return app.run(request);
  },
};
