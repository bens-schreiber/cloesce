import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, stopWrangler, withRes } from "../src/setup";
import { Hamburger, Topping } from "../fixtures/adv_ds/client";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("./fixtures/adv_ds");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Advanced Data Sources", () => {
  let burgers: Hamburger[] = [];
  let baconTopping: Topping;
  let lettuceTopping: Topping;
  it("POST hamburgers with various toppings", async () => {
    const bacon = Topping.SAVE({ name: "BACON" });
    const lettuce = Topping.SAVE({ name: "LETTUCE" });
    const tomato = Topping.SAVE({ name: "TOMATO" });

    const res = await Promise.all([bacon, lettuce, tomato]);
    expect(res.every((r) => r.ok)).toBe(true);
    baconTopping = res[0].data!;
    lettuceTopping = res[1].data!;

    const burger1 = Hamburger.SAVE({
      name: "bacon lettuce burger",
      toppings: [baconTopping, lettuceTopping],
    });
    const burger2 = Hamburger.SAVE({
      name: "lettuce burger",
      toppings: [lettuceTopping],
    });
    const burger3 = Hamburger.SAVE({
      name: "bacon burger",
      toppings: [baconTopping],
    });
    const burger4 = Hamburger.SAVE({ name: "plain burger", toppings: [] });

    const burgerRes = await Promise.all([burger1, burger2, burger3, burger4]);
    expect(burgerRes.every((r) => r.ok)).toBe(true);
    burgers = burgerRes.map((r) => r.data!);
  });

  it("LIST all hamburgers with default", async () => {
    const res = await Hamburger.LIST();
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toBe(4);
  });

  it("LIST hamburgers with 'orderedBurgersWithLettuce' data source", async () => {
    const res = await Hamburger.LIST("orderedBurgersWithLettuce");
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toBe(2);
    expect(res.data![0].id).toBe(burgers[0].id);
    expect(res.data![1].id).toBe(burgers[1].id);
  });

  it("`noLettuceToppings` should return only the toppings that arent LETTUCE", async () => {
    const res = await burgers[0].noLettuceToppings();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data!.length).toBe(1);
    expect(res.data![0].name).toBe("BACON");
  });

  it("`onlyBaconToppings` should return only the toppings that are BACON", async () => {
    const res = await burgers[0].onlyBacon();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data!.length).toBe(1);
    expect(res.data![0].name).toBe("BACON");
  });
});
