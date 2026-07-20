import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Global, SubReddit, Post, Comment } from "../fixtures/durable_objects/client";
import config from "../fixtures/durable_objects/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/durable_objects", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Global Durable Object", () => {
  it("newGlobal returns a fresh model from the static API method", async () => {
    const res = await Global.newGlobal();
    expectHttpResult(res, "newGlobal should be OK");
    expect(res.data).toBeInstanceOf(Global);
  });

  it("getMetadata instance method runs inside the global DO", async () => {
    const created = await Global.newGlobal();
    expectHttpResult(created, "newGlobal should be OK");

    // getMetadata reads the global DO's `metadata` KV, which is hydrated inside the
    // DO from its own storage (unset here, so it resolves to an empty value).
    const meta = await created.data!.getMetadata();
    expectHttpResult(meta, "getMetadata should be OK");
  });
});

describe("Sharded Durable Object-backed model", () => {
  it("newSubReddit returns a fresh model from the static API method", async () => {
    const res = await SubReddit.newSubReddit();
    expectHttpResult(res, "newSubReddit should be OK");
    expect(res.data).toBeInstanceOf(SubReddit);
  });

  it("$save fans out KV writes to the DO storage and a Worker KV namespace", async () => {
    const saved = await SubReddit.$save(1, {
      subId: 1,
      metadata: "r/cloesce",
      globalMetadata: { raw: "global-1" },
    });
    expectHttpResult(saved, "$save should be OK");

    const got = await SubReddit.$get(1);
    expectHttpResult(got, "$get should be OK");
    expect(got.data!.subId).toBe(1);

    expect(got.data!.metadata).toBe("r/cloesce");
    expect(got.data!.globalMetadata.value).toBe("global-1");
  });

  it("different shards resolve to isolated DO instances", async () => {
    await SubReddit.$save(2, {
      subId: 2,
      metadata: "r/other",
      globalMetadata: { raw: "global-2" },
    });

    const sub2 = await SubReddit.$get(2);
    expect(sub2.data!.metadata).toBe("r/other");

    // shard 1 is unaffected by shard 2's write.
    const sub1 = await SubReddit.$get(1);
    expect(sub1.data!.metadata).toBe("r/cloesce");
  });

  it("rejects a subId that violates the inherited shard validator", async () => {
    // subId inherits `[gt 0]` from the shard field.
    const res = await SubReddit.$get(0);
    expect(res.ok, `$get(0) should fail validation\n\n${JSON.stringify(res)}`).toBe(false);
    expect(res.status).toBe(400);
  });
});

describe("SQL-backed Durable Object model (Post)", () => {
  it("$save inserts a row into the DO's SQLite database (migration applied on construction)", async () => {
    const saved = await Post.$save(1, { title: "first", content: "hello" });
    expectHttpResult(saved, "$save should be OK");
    expect(saved.data!.id).toBeTypeOf("number");
    expect(saved.data!.title).toBe("first");
    expect(saved.data!.content).toBe("hello");
    expect(saved.data!.subId).toBe(1);
  });

  it("$get fetches a row by primary key inside the DO", async () => {
    const saved = await Post.$save(1, { title: "second", content: "world" });
    expectHttpResult(saved, "$save should be OK");

    const got = await Post.$get(1, saved.data!.id);
    expectHttpResult(got, "$get should be OK");
    expect(got.data!.id).toBe(saved.data!.id);
    expect(got.data!.title).toBe("second");
    expect(got.data!.subId).toBe(1);
  });

  it("$save with a primary key updates the existing row", async () => {
    const saved = await Post.$save(1, { title: "draft", content: "v1" });
    expectHttpResult(saved, "$save should be OK");

    const updated = await Post.$save(1, {
      id: saved.data!.id,
      title: "draft",
      content: "v2",
    });
    expectHttpResult(updated, "$save update should be OK");
    expect(updated.data!.id).toBe(saved.data!.id);
    expect(updated.data!.content).toBe("v2");
  });

  it("$list seek-paginates rows within the DO", async () => {
    const all = await Post.$list(1, 0, 100);
    expectHttpResult(all, "$list should be OK");
    expect(all.data!.length).toBeGreaterThanOrEqual(3);

    const firstId = all.data![0].id;
    const rest = await Post.$list(1, firstId, 100);
    expectHttpResult(rest, "$list after firstId should be OK");
    expect(rest.data!.length).toBe(all.data!.length - 1);
    expect(rest.data!.every((row) => row.id > firstId)).toBe(true);
  });

  it("rows are isolated per shard", async () => {
    await Post.$save(9, { title: "shard9", content: "isolated" });

    const shard9 = await Post.$list(9, 0, 100);
    expectHttpResult(shard9, "$list(9) should be OK");
    expect(shard9.data!.length).toBe(1);
    expect(shard9.data![0].title).toBe("shard9");

    // shard 9's database does not contain shard 1's rows.
    expect(shard9.data!.some((row) => row.title === "first")).toBe(false);
  });

  it("$get for a missing row returns 404", async () => {
    const res = await Post.$get(1, 999999);
    expect(res.ok, `expected 404\n\n${JSON.stringify(res)}`).toBe(false);
    expect(res.status).toBe(404);
  });
});

describe("SQL-backed Durable Object model with a foreign key (Comment)", () => {
  it("a Comment can reference an existing Post via its postId foreign key", async () => {
    const post = await Post.$save(1, { title: "with comments", content: "body" });
    expectHttpResult(post, "post $save should be OK");

    const comment = await Comment.$save(1, {
      content: "nice post",
      upvotes: 3,
      postId: post.data!.id,
    });
    expectHttpResult(comment, "comment $save should be OK");
    expect(comment.data!.postId).toBe(post.data!.id);
    expect(comment.data!.upvotes).toBe(3);

    const got = await Comment.$get(1, comment.data!.id);
    expectHttpResult(got, "comment $get should be OK");
    expect(got.data!.content).toBe("nice post");
  });

  it("$get hydrates a Post's comments navigation property", async () => {
    const post = await Post.$save(1, { title: "navtest", content: "body" });
    expectHttpResult(post, "post $save should be OK");

    await Comment.$save(1, { content: "c1", upvotes: 1, postId: post.data!.id });
    await Comment.$save(1, { content: "c2", upvotes: 2, postId: post.data!.id });

    const got = await Post.$get(1, post.data!.id);
    expectHttpResult(got, "post $get should be OK");
    expect(got.data!.comments.length).toBe(2);
    expect(got.data!.comments.map((c) => c.content).sort()).toEqual(["c1", "c2"]);
  });
});

describe("Injected Durable Object instance method (feed)", () => {
  it("feed runs inside the SubRedditDo and lists its Posts", async () => {
    const sub = await SubReddit.$get(1);
    expectHttpResult(sub, "$get should be OK");

    const feed = await sub.data!.feed();
    expectHttpResult(feed, "feed should be OK");
    expect(feed.data!.length).toBeGreaterThanOrEqual(1);
    expect(feed.data!.every((p) => p instanceof Post)).toBe(true);
  });
});

describe("Custom data source reaching the DO over RPC", () => {
  it("$save_Custom round-trips through the shard's SQLite via an RPC method", async () => {
    const saved = await Post.$save_Custom({ title: "rpc-post", content: "via rpc" }, 1);
    expectHttpResult(saved, "$save_Custom should be OK");
    expect(saved.data!.id).toBeTypeOf("number");
    expect(saved.data!.subId).toBe(1);

    const got = await Post.$get_Custom(saved.data!.id, 1);
    expectHttpResult(got, "$get_Custom should be OK");
    expect(got.data!.title).toBe("rpc-post");
    expect(got.data!.subId).toBe(1);
  });

  it("$list_Custom lists the shard's Posts over RPC", async () => {
    const list = await Post.$list_Custom(1);
    expectHttpResult(list, "$list_Custom should be OK");
    expect(list.data!.length).toBeGreaterThanOrEqual(1);
    expect(list.data!.some((p) => p.title === "rpc-post")).toBe(true);
  });

  it("$get_Custom for a missing row returns 404", async () => {
    const res = await Post.$get_Custom(999999, 1);
    expect(res.ok, `expected 404\n\n${JSON.stringify(res)}`).toBe(false);
    expect(res.status).toBe(404);
  });
});
