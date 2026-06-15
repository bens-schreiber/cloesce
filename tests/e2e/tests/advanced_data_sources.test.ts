import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { expectHttpResult, startWrangler } from "../src/setup";
import { DefaultOverride, Hamburger, Topping } from "../fixtures/adv_ds/client";
import config from "../fixtures/adv_ds/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/adv_ds", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Advanced Data Sources", () => {
  let burgers: Hamburger[] = [];
  let baconTopping: Topping;
  let lettuceTopping: Topping;
  it("POST hamburgers with various toppings", async () => {
    const bacon = Topping.$save({ name: "BACON" });
    const lettuce = Topping.$save({ name: "LETTUCE" });
    const tomato = Topping.$save({ name: "TOMATO" });

    const res = await Promise.all([bacon, lettuce, tomato]);
    expect(res.every((r) => r.ok)).toBe(true);
    baconTopping = res[0].data!;
    lettuceTopping = res[1].data!;

    const burger1 = Hamburger.$save({
      name: "bacon lettuce burger",
      toppings: [
        {
          toppingId: baconTopping.id,
        },
        {
          toppingId: lettuceTopping.id,
        },
      ],
    });
    const burger2 = Hamburger.$save({
      name: "lettuce burger",
      toppings: [
        {
          toppingId: lettuceTopping.id,
        },
      ],
    });
    const burger3 = Hamburger.$save({
      name: "bacon burger",
      toppings: [
        {
          toppingId: baconTopping.id,
        },
      ],
    });
    const burger4 = Hamburger.$save({ name: "plain burger", toppings: [] });

    const burgerRes = await Promise.all([burger1, burger2, burger3, burger4]);
    expect(burgerRes.every((r) => r.ok)).toBe(true);
    burgers = burgerRes.map((r) => r.data!);
  });

  it("$list all hamburgers with default", async () => {
    const res = await Hamburger.$list(0, 100);

    expectHttpResult(res, "$list should be OK");
    expect(res.data!.length).toBe(4);
  });

  it("$list hamburgers with BurgersWithLettuceOrdered data source", async () => {
    const res = await Hamburger.$list_BurgersWithLettuceOrdered(0, 100);

    expectHttpResult(res, "$list should be OK");
    expect(res.data!.length).toBe(2);
    expect(res.data![0].id).toBe(burgers[0].id);
    expect(res.data![1].id).toBe(burgers[1].id);
  });

  it("`noLettuceToppings` should return only the toppings that arent LETTUCE", async () => {
    const res = await burgers[0].noLettuceToppings();
    expectHttpResult(res, "GET should be OK");
    expect(res.data!.length).toBe(1);
    expect(res.data![0].name).toBe("BACON");
  });

  it("`onlyBaconToppings` should return only the toppings that are BACON", async () => {
    const res = await burgers[0].onlyBaconToppings();
    expectHttpResult(res, "GET should be OK");
    expect(res.data!.length).toBe(1);
    expect(res.data![0].name).toBe("BACON");
  });

  it("DefaultOverrides default data source returns fixed values", async () => {
    const res = await DefaultOverride.$get();
    expectHttpResult(res, "GET should be OK");
    expect(res.data!.id).toBe(Number.MAX_VALUE);

    const listRes = await DefaultOverride.$list();
    expectHttpResult(listRes, "LIST should be OK");
    expect(listRes.data!.length).toBe(1);
    expect(listRes.data![0].id).toBe(Number.MAX_VALUE);
  });
});
