import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { readFile } from "fs/promises";
import { startWrangler, expectHttpResult } from "../src/setup.js";
import { DeckOfManyThings, VoxMachina } from "../fixtures/internal/client";
import config from "../fixtures/internal/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/internal", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Internal models and plain-old-objects never reach the client", () => {
  it("omits the internal `Card` poo and Deck instance shape from the generated client entirely", async () => {
    const clientSource = await readFile(
      new URL("../fixtures/internal/client.ts", import.meta.url),
      "utf8",
    );

    expect(clientSource).not.toContain("Card");
    expect(clientSource).not.toContain("isCursed");

    // An internal model with a static API still gets a class (so its static method is
    // callable), but it must carry none of the instance machinery a public model would
    const deckClassMatch = clientSource.match(/export class DeckOfManyThings \{([\s\S]*?)\n\}/);
    expect(deckClassMatch).not.toBeNull();
    const deckClassBody = deckClassMatch![1];
    expect(deckClassBody).not.toContain("id:");
    expect(deckClassBody).not.toContain("fromJson");
  });

  it("still lets VoxMachina, a public model, generate normally", async () => {
    const clientSource = await readFile(
      new URL("../fixtures/internal/client.ts", import.meta.url),
      "utf8",
    );
    expect(clientSource).toContain("export class VoxMachina");
  });
});

describe("Calling the internal model's static API", () => {
  it("draws from the Deck of Many Things without ever exposing a Card to the client", async () => {
    const res = await DeckOfManyThings.draw("Vex'ahlia");
    expectHttpResult(res, "draw should be OK");
    expect(typeof res.data).toBe("boolean");
  });

  it("draws consistently for the same name (deterministic server-side selection)", async () => {
    const first = await DeckOfManyThings.draw("Percival");
    const second = await DeckOfManyThings.draw("Percival");
    expectHttpResult(first);
    expectHttpResult(second);
    expect(first.data).toEqual(second.data);
  });
});

describe("Public models are unaffected by internal structures elsewhere in the schema", () => {
  it("saves and fetches a member of Vox Machina", async () => {
    const res = await VoxMachina.$save({ name: "Vax'ildan" });
    expectHttpResult(res, "POST should be OK");
    expect(res.data).toEqual({ id: 1, name: "Vax'ildan" });

    const fetched = await VoxMachina.$get(res.data!.id);
    expectHttpResult(fetched, "GET should be OK");
    expect(fetched.data).toEqual({ id: 1, name: "Vax'ildan" });
  });
});
