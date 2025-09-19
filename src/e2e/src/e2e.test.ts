import { compile, linkGeneratedModule, startWrangler } from "./setup.js";

let Horse: any;
let Match: any;

describe("E2E Tests", () => {
  beforeAll(async () => {
    compile();
    await startWrangler();

    let mod = await linkGeneratedModule();
    expect(mod.Horse).toBeDefined();
    expect(mod.Match).toBeDefined();

    Horse = mod.Horse;
    Match = mod.Match;
  }, 30_000);

  it("Post, Patch, Get a Horse", async () => {
    let body = {
      id: 0,
      name: "roach",
      bio: "geralts horse",
      matches: [],
    };

    let res = await Horse.post(body);
    expect(res.ok);
    expect(res.data).toEqual(body);

    body.name = "ROACH";
    res = await Horse.patch(body);
    expect(res.ok);

    res = await Horse.get(body.id);
    expect(res.ok);
    expect(res.data).toEqual(body);
  });

  it("List horse returns all horses", async () => {
    let res = await Horse.list();
    expect(res.ok);
    let horses = res.data;
    expect(horses.length).toBe(1);

    let newHorses = [
      {
        id: 2,
        name: "sonic",
        bio: "the horse",
        matches: [],
      },
      {
        id: 3,
        name: "other roach",
        bio: "geralts other horse",
        matches: [],
      },
    ];

    let postResults = await Promise.all(newHorses.map((h) => Horse.post(h)));
    postResults.forEach((res) => {
      expect(res.ok).toBe(true);
    });

    res = await Horse.list();
    expect(res.ok).toBe(true);
    expect(res.data.length).toBe(3);
    expect(res.data).toEqual(expect.arrayContaining([...horses, ...newHorses]));
  });

  it("Horse can match with another horse", async () => {
    let res = await Horse.get(0);
    expect(res.ok).toBe(true);
    let horse1 = res.data;

    res = await Horse.get(1);
    expect(res.ok).toBe(true);
    let horse2 = res.data;

    res = await horse1.match(horse2);
    expect(res.ok).toBe(true);

    res = await Horse.get(horse1.id);
    expect(res.ok).toBe(true);
    let updated_horse1 = res.data;

    res = await Horse.get(horse2.id);
    expect(res.ok).toBe(true);
    let updated_horse2 = res.data;

    expect(horse1.matches.length).toBe(1);
    expect(horse2.matches.length).toBe(1);
    expect(updated_horse1.matches).toContain(horse2.id);
    expect(updated_horse2.matches).toContain(horse1.id);
  });

  it("Default include tree shows all matches but goes no further", async () => {
    let res = await Horse.get(0);
    expect(res.ok).toBe(true);
    let horse1 = res.data;
    expect(horse1.matches[0].matches.length).toBe(0);
  });
});
