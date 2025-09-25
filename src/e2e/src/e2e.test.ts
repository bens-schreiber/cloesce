import test, { after, before } from "node:test";
import assert from "node:assert/strict";
import {
  compile,
  linkGeneratedModule,
  startWrangler,
  stopWrangler,
} from "./setup.ts";

function withRes(message: string, res: any): string {
  return `${message}\n\n${JSON.stringify(res, null, 2)}`;
}

let Horse: any;
let Like: any;

before(
  async () => {
    compile();
    await startWrangler();

    const mod = await linkGeneratedModule();
    assert.ok(mod.Horse);
    assert.ok(mod.Like);

    Horse = mod.Horse;
    Like = mod.Like;
  },
  { timeout: 30_000 }
);

after(async () => {
  await stopWrangler();
});

test("Post, Patch, Get a Horse", async () => {
  let body = {
    id: 0,
    name: "roach",
    bio: "geralts horse",
    likes: [],
  };

  let res = await Horse.post(body);
  assert.ok(res.ok, withRes("POST should be OK", res));
  assert.ok(
    res.data.id == body.id,
    withRes("POST response id should be the same as the inputted id", res)
  );

  body.name = "ROACH";
  let horse = Object.assign(new Horse(), body);
  res = await horse.patch(body);
  assert.ok(res.ok, withRes("PATCH should be OK", res));

  res = await Horse.get(body.id);
  assert.ok(res.ok, withRes("GET should be OK", res));

  horse = res.data;
  assert.ok(
    horse.id == body.id,
    withRes("GET response id should be the same as the inputted id", res)
  );
});

test("List horse returns all horses", async () => {
  let res = await Horse.list();
  assert.ok(res.ok);
  let horses = res.data;
  assert.equal(horses.length, 1);

  let newHorses = [
    {
      id: 1,
      name: "sonic",
      bio: "the horse",
      likes: [],
    },
    {
      id: 2,
      name: "other roach",
      bio: "geralts other horse",
      likes: [],
    },
  ];

  let postResults = await Promise.all(newHorses.map((h) => Horse.post(h)));
  postResults.forEach((res) => assert.ok(res.ok));

  res = await Horse.list();
  assert.ok(res.ok);
  assert.equal(res.data.length, 3);

  // Node's assert doesn't have `arrayContaining`, so use deepEqual after sorting
  const allHorses = [...horses, ...newHorses];

  const normalize = (arr: any[]) =>
    arr.sort((a, b) => a.id - b.id).map((h) => ({ ...h })); // strips prototype so Horse === plain object

  assert.deepEqual(normalize(res.data), normalize(allHorses));
});

test("Horse can like another horse", async () => {
  // Arrange
  let res = await Horse.get(0);
  assert.ok(res.ok, withRes("GET response should be OK", res));
  let horse1 = Object.assign(new Horse(), res.data);

  res = await Horse.get(1);
  assert.ok(res.ok, withRes("GET response should be OK", res));
  let horse2 = Object.assign(new Horse(), res.data);

  // Act
  res = await horse1.like(horse2);
  assert.ok(res.ok, withRes(".like() response should be OK", res));

  res = await Horse.get(horse1.id);
  assert.ok(res.ok, withRes("GET response should be OK", res));
  let updated_horse1 = res.data;

  res = await Horse.get(horse2.id);
  assert.ok(res.ok, withRes("GET response should be OK", res));
  let updated_horse2 = res.data;

  // Assert
  assert.equal(updated_horse1.likes.length, 1);
  assert.equal(updated_horse2.likes.length, 0);
  assert.ok(updated_horse1.likes.find((l: any) => l.horseId2 == horse2.id));
});

test("Default include tree shows all likes but goes no further", async () => {
  let res = await Horse.get(0);
  assert.ok(res.ok);
  let horse1 = res.data;
  assert.notEqual(horse1.likes[0].horse2, undefined);
  assert.ok(horse1.likes[0].horse2.likes.length == 0);
});
