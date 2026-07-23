import {
  createApp,
  Worker,
  Hamburger,
  Topping,
  HamburgerTopping,
  DefaultOverride,
  type Api,
  type CfEnv,
} from "./backend.js";
import { HttpResult } from "cloesce";

const noLettuce: Api.Hamburger.NoLettuce = {
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

    return env.db.hamburger.noLettuce.hydrate({ ...burger, toppings });
  },
};

const onlyBacon: Api.Hamburger.OnlyBacon = {
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
    return env.db.hamburger.onlyBacon.hydrate({ ...burger, toppings });
  },
};

const burgersWithLettuceOrdered: Api.Hamburger.BurgersWithLettuceOrdered = {
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
    return env.db.hamburger.burgersWithLettuceOrdered.hydrateAll(rows);
  },
};

const hamburger: Api.Hamburger.Of = {
  noLettuceToppings(self) {
    return self.toppings.map((t) => t.topping);
  },

  onlyBaconToppings(self) {
    return self.toppings.map((t) => t.topping);
  },

  BurgersWithLettuceOrdered: burgersWithLettuceOrdered,
  NoLettuce: noLettuce,
  OnlyBacon: onlyBacon,
};

const defaultOverride: Api.DefaultOverride.Of = {
  Default: {
    get() {
      return HttpResult.ok(200, { id: Number.MAX_VALUE });
    },
    list() {
      return HttpResult.ok(200, [{ id: Number.MAX_VALUE }]);
    },
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker)
      .register(Hamburger, hamburger)
      .register(Topping, {})
      .register(HamburgerTopping, {})
      .register(DefaultOverride, defaultOverride)
      .run(request);
  },
};
