import { startWrangler, stopWrangler } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Horse } from "../../fixtures/horse_tinder/client.js";

function withRes(message: string, res: any): string {
  return `${message}\n\n${JSON.stringify(res, null, 2)}`;
}

beforeAll(async () => {
  await startWrangler("../fixtures/horse_tinder");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("POST, GET a Horse", () => {
  const roach = new Horse();
  roach.id = 0;
  roach.name = "Roach";
  roach.bio = "Geralt's horse";
  it("POST a Horse", async () => {
    // Act
    const res = await Horse.post(roach);

    // Assert
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(
      res.data.id,
      withRes("POST response id should match input id", res),
    ).toBe(roach.id);
  });

  let horse: Horse;
  it("GET a Horse", async () => {
    const res = await Horse.get(roach.id);
    horse = res.data;

    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(
      horse.id,
      withRes("GET response id should match input id", res),
    ).toBe(roach.id);
  });
});

describe("List tests", () => {
  let initialHorses: Horse[] = [];
  it("should return initial list of horses", async () => {
    const res = await Horse.list();
    expect(res.ok).toBe(true);
    initialHorses = res.data;
    expect(initialHorses.length).toBe(1);
  });

  let newHorses: Horse[] = [];
  it("should add new horses", async () => {
    newHorses = [
      { id: 1, name: "sonic", bio: "the horse", likes: [] },
      { id: 2, name: "other roach", bio: "geralt's other horse", likes: [] },
    ].map((v) => Object.assign(new Horse(), v));

    const postResults = await Promise.all(newHorses.map((h) => Horse.post(h)));
    postResults.forEach((res) => expect(res.ok).toBe(true));
  });

  it("should list all horses including newly added ones", async () => {
    const res = await Horse.list();
    expect(res.ok).toBe(true);

    const allHorses = [...initialHorses, ...newHorses];

    const normalize = (arr: Horse[]) =>
      arr
        .slice()
        .sort((a, b) => a.id - b.id)
        .map((h) => ({ ...h }));

    expect(normalize(res.data)).toEqual(normalize(allHorses));
  });
});

describe("Horse.like", () => {
  let horse1: Horse;
  let horse2: Horse;

  it("should fetch horse 0", async () => {
    const res = await Horse.get(0);
    expect(res.ok).toBe(true);
    horse1 = Object.assign(new Horse(), res.data);
  });

  it("should fetch horse 1", async () => {
    const res = await Horse.get(1);
    expect(res.ok).toBe(true);
    horse2 = Object.assign(new Horse(), res.data);
  });

  it("horse1 should like horse2", async () => {
    const res = await horse1.like(horse2);
    expect(res.ok).toBe(true);
  });

  it("should fetch updated horse1", async () => {
    const res = await Horse.get(horse1.id);
    expect(res.ok).toBe(true);
    horse1 = Object.assign(new Horse(), res.data);
  });

  it("should fetch updated horse2", async () => {
    const res = await Horse.get(horse2.id);
    expect(res.ok).toBe(true);
    horse2 = Object.assign(new Horse(), res.data);
  });

  it("should verify likes", () => {
    expect(horse1.likes.length).toBe(1);
    expect(horse2.likes.length).toBe(0);

    const likeExists = horse1.likes.some((l: any) => l.horseId2 === horse2.id);
    expect(likeExists).toBe(true);
  });
});

describe("Horse default include tree", () => {
  let horse1: Horse;

  it("should fetch horse 0", async () => {
    const res = await Horse.get(0);
    expect(res.ok).toBe(true);
    horse1 = Object.assign(new Horse(), res.data);
  });

  it("should include likes but not further likes", () => {
    const firstLike = horse1.likes[0];

    // The liked horse should exist
    expect(firstLike.horse2).not.toBeUndefined();

    // The liked horse should have no likes loaded by default
    expect(firstLike.horse2?.likes.length).toBe(0);
  });
});
