import * as clo from "./backend.js";
import { HttpResult } from "cloesce";

// No raw SQL: hydrate the burger with this data source's own include tree
// (toppings.topping), then filter the toppings list in JS.
async function getFilteringToppings(
  env: { db: clo.Env.db },
  id: number,
  tree: unknown,
  keep: (name: string) => boolean,
): Promise<HttpResult<clo.Hamburger.Self | null>> {
  const res = await clo.Hamburger.Orm.get(env, { id, include: tree as never });
  if (res.errors.length > 0) {
    return HttpResult.fail(400, JSON.stringify(res.errors));
  }
  if (res.value === null) {
    return HttpResult.fail(404);
  }
  res.value.toppings = res.value.toppings.filter((t) => keep(t.topping!.name));
  return HttpResult.ok(200, res.value);
}

const NoLettuce = clo.Hamburger.NoLettuce.impl({
  async get(env, id) {
    return await getFilteringToppings(env, id, this.tree, (name) => name !== "LETTUCE");
  },
});

const OnlyBacon = clo.Hamburger.OnlyBacon.impl({
  async get(env, id) {
    return await getFilteringToppings(env, id, this.tree, (name) => name === "BACON");
  },
});

const BurgersWithLettuceOrdered = clo.Hamburger.BurgersWithLettuceOrdered.impl({
  async list(env, lastId, limit) {
    // Fetch the seek-filtered (id > lastId) ascending page, hydrating each
    // burger with this data source's tree, then keep only burgers with a
    // LETTUCE topping and cap to `limit` in JS.
    const page = await clo.Hamburger.GeneratedSource.Default.list(
      env,
      lastId,
      Number.MAX_SAFE_INTEGER,
    );
    if (!page.ok) {
      return page;
    }
    const hydrated = await Promise.all(page.data!.map((b) => this.get(env, b.id)));
    const failed = hydrated.find((r) => !r.ok);
    if (failed) {
      return failed as HttpResult<never>;
    }
    const burgers = hydrated
      .map((r) => r.data!)
      .filter((b) => b.toppings.some((t) => t.topping!.name === "LETTUCE"))
      .slice(0, limit);
    return HttpResult.ok(200, burgers);
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
