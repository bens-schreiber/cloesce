import test, { before } from "node:test";
import assert from "node:assert/strict";
import { compile, linkGeneratedModule, startWrangler } from "./setup.ts";

let Horse: any;
let Match: any;

before(
  async () => {
    compile();
    await startWrangler();

    const mod = await linkGeneratedModule();
    assert.ok(mod.Horse);
    assert.ok(mod.Match);

    Horse = mod.Horse;
    Match = mod.Match;
  },
  { timeout: 30_000 }
);

test("Post, Patch, Get a Horse", async () => {
  let body = {
    id: 0,
    name: "roach",
    bio: "geralts horse",
    matches: [],
  };
  let res = await Horse.post(body);
  assert.ok(res.ok, "POST should be OK");
  assert.ok(
    res.data.id == body.id,
    "POST response id should be the same as the inputted id"
  );

  body.name = "ROACH";
  let horse = Object.assign(new Horse(), body);
  res = await horse.patch(body);
  assert.ok(res.ok, "PATCH should be OK");

  res = await Horse.get(body.id);
  console.log(res);
  assert.ok(res.ok, "GET should be OK");
  assert.ok(
    res.data.id == body.id,
    "GET response id should be the same as the inputted id"
  );
});

test("List horse returns all horses", async () => {
  let res = await Horse.list();
  assert.ok(res.ok);
  let horses = res.data;
  assert.equal(horses.length, 1);

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
  postResults.forEach((res) => assert.ok(res.ok));

  res = await Horse.list();
  assert.ok(res.ok);
  assert.equal(res.data.length, 3);

  // Node's assert doesn't have `arrayContaining`, so use deepEqual after sorting
  const allHorses = [...horses, ...newHorses];
  assert.deepEqual(
    res.data.sort((a: any, b: any) => a.id - b.id),
    allHorses.sort((a: any, b: any) => a.id - b.id)
  );
});

test("Horse can match with another horse", async () => {
  let res = await Horse.get(0);
  assert.ok(res.ok);
  let horse1 = res.data;

  res = await Horse.get(1);
  assert.ok(res.ok);
  let horse2 = res.data;

  res = await horse1.match(horse2);
  assert.ok(res.ok);

  res = await Horse.get(horse1.id);
  assert.ok(res.ok);
  let updated_horse1 = res.data;

  res = await Horse.get(horse2.id);
  assert.ok(res.ok);
  let updated_horse2 = res.data;

  assert.equal(horse1.matches.length, 1);
  assert.equal(horse2.matches.length, 1);
  assert.ok(updated_horse1.matches.includes(horse2.id));
  assert.ok(updated_horse2.matches.includes(horse1.id));
});

test("Default include tree shows all matches but goes no further", async () => {
  let res = await Horse.get(0);
  assert.ok(res.ok);
  let horse1 = res.data;
  assert.equal(horse1.matches[0].matches.length, 0);
});
