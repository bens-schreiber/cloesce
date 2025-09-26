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

// Because we link the files at runtime, types must be declared like this.
let Horse: any;

before(
  async () => {
    compile();
    await startWrangler();

    const mod = await linkGeneratedModule();
    assert.ok(mod.Horse);
    assert.ok(mod.Like);
    Horse = mod.Horse;
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

  // POST
  {
    // Act
    let res = await Horse.post(body);

    // Assert
    assert.ok(res.ok, withRes("POST should be OK", res));
    assert.ok(
      res.data.id == body.id,
      withRes("POST response id should be the same as the inputted id", res)
    );
  }

  // GET
  let horse = undefined;
  {
    // Act
    let res = await Horse.get(body.id);
    horse = res.data;

    // Assert
    assert.ok(res.ok, withRes("GET should be OK", res));
    assert.ok(
      horse.id == body.id,
      withRes("GET response id should be the same as the inputted id", res)
    );
  }

  // PATCH
  {
    body.name = "ROACH";

    // Act
    let res = await horse.patch(body);

    // Assert
    assert.ok(res.ok, withRes("PATCH should be OK", res));
  }
});

test("List horse returns all horses", async () => {
  // Initial List
  let horses = undefined;
  {
    // Act
    let res = await Horse.list();

    // Assert
    assert.ok(res.ok, withRes("Expected List to be OK", res));
    horses = res.data;
    assert.equal(horses.length, 1);
  }

  // Updated List
  {
    // Arrange
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

    // Act
    let res = await Horse.list();

    // Assert
    assert.ok(res.ok, withRes("List should be OK", res));
    assert.equal(res.data.length, 3);

    const allHorses = [...horses, ...newHorses];
    const normalize = (arr: any[]) =>
      arr.sort((a, b) => a.id - b.id).map((h) => ({ ...h })); // strips prototype so Horse === plain object
    assert.deepEqual(normalize(res.data), normalize(allHorses));
  }
});

test("Horse can like another horse", async () => {
  // Arrange
  let res = await Horse.get(0);
  assert.ok(res.ok, withRes("GET response should be OK", res));
  let horse1 = res.data;

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
  assert.ok(
    updated_horse1.likes.find((l: any) => l.horseId2 == horse2.id),
    `${JSON.stringify(updated_horse1)}`
  );
});

test("Default include tree shows all likes but goes no further", async () => {
  // Act
  let res = await Horse.get(0);
  assert.ok(res.ok, withRes("GET should be OK", res));
  let horse1 = res.data;

  // Assert
  assert.notEqual(horse1.likes[0].horse2, undefined);
  assert.ok(horse1.likes[0].horse2.likes.length == 0);
});

test("Methods can return both data and errors", async () => {
  // Err
  {
    let res = await Horse.divide(1, 0);
    assert.deepEqual(
      res,
      { ok: false, status: 400, message: "divided by 0" },
      withRes("Divide by zero should produce an error", res)
    );
  }

  // No err
  {
    let res = await Horse.divide(1, 1);
    assert.equal(
      res.data,
      1,
      withRes(
        "Divide by 1 should not produce an error and give an integer result",
        res
      )
    );
  }
});
