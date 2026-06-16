import { describe, expect, it } from "vitest";
import { upgradeEnv } from "../.cloesce/backend.js";
import { Comment, Post, SubReddit, User } from "../src/api/main.js";
import { env, inSub, inUser } from "./setup.js";

describe("Auth", () => {
  it("login claims a username and returns a token + profile", async () => {
    const res = await inUser("alice", null, (e) => User.login(e, "alice"));
    expect(res.token).toBeTypeOf("string");
    expect(res.user.username).toBe("alice");
  });

  it("posting while anonymous is 401", async () => {
    const res = await inSub("s1", null, (e) => Post.create(e, "s1", "anon", "nope"));
    expect(res.status).toBe(401);
  });
});

describe("Subreddits", () => {
  it("a logged-in user can create one (server assigns its id)", async () => {
    const sub = await inSub("ignored", "alice", (e) =>
      SubReddit.create(e, { name: "r/dogs", description: "woof" }),
    );
    expect(sub.subId).toBeTypeOf("string");
    expect(sub.metadata.value.name).toBe("r/dogs");
  });

  it("created subreddits appear in the global directory", async () => {
    const sub = await inSub("ignored", "alice", (e) =>
      SubReddit.create(e, { name: "r/listed", description: "" }),
    );
    const dir = await SubReddit.list({ SubReddits: upgradeEnv(env).SubReddits });
    expect(dir.results.map((s: any) => s.subId)).toContain(sub.subId);
    expect(dir.results.find((s: any) => s.subId === sub.subId)!.name).toBe("r/listed");
  });
});

describe("Posts, comments, and the feed", () => {
  it("create a post, comment on it, then read it back with its comments", async () => {
    const post = await inSub("r1", "alice", (e) => Post.create(e, "r1", "Cats", "Discuss."));
    expect(post.author).toBe("alice");
    expect(post.upvotes).toBe(0);

    await inSub("r1", "alice", (e) => Comment.create(e, "r1", post.id, "agreed!"));

    const view = (await inSub("r1", "alice", (e) => Post.Default.get(e, "r1", post.id))).data!;
    expect(view.comments.map((c: any) => c.content)).toContain("agreed!");
  });

  it("the feed lists a subreddit's posts, isolated per shard", async () => {
    await inSub("r1", "alice", (e) => Post.create(e, "r1", "in r1", "body"));
    const feed1 = await inSub("r1", "alice", (e) => SubReddit.feed(null as any, e, "r1"));
    const feed2 = await inSub("r2", "alice", (e) => SubReddit.feed(null as any, e, "r2"));
    expect(feed1.length).toBeGreaterThanOrEqual(1);
    expect(feed2.length).toBe(0);
  });
});

describe("Voting", () => {
  it("up then down nets to zero on a post; up works on a comment", async () => {
    // vote is an instance method: pass the latest model back in each time.
    const post = await inSub("r1", "alice", (e) => Post.create(e, "r1", "vote", "body"));
    const up = await inSub("r1", "alice", (e) => Post.vote(post, e, "r1", 1));
    expect(up.upvotes).toBe(1);
    expect((await inSub("r1", "alice", (e) => Post.vote(up, e, "r1", -1))).upvotes).toBe(0);

    const c = await inSub("r1", "alice", (e) => Comment.create(e, "r1", post.id, "hi"));
    expect((await inSub("r1", "alice", (e) => Comment.vote(c, e, "r1", 1))).upvotes).toBe(1);
  });
});

describe("Profile activity", () => {
  it("creating things records them in the user's profile", async () => {
    await inUser("dana", null, (e) => User.login(e, "dana"));
    const sub = await inSub("ignored", "dana", (e) =>
      SubReddit.create(e, { name: "r/d", description: "d" }),
    );
    const post = await inSub("r9", "dana", (e) => Post.create(e, "r9", "p", "b"));
    await inSub("r9", "dana", (e) => Comment.create(e, "r9", post.id, "c"));

    const profile = await inUser("dana", "dana", (e) => e.ctx.profile.get());
    expect(profile.subReddits).toContain(sub.subId);
    expect(profile.posts).toContain(post.id);
    expect(profile.comments.length).toBe(1);
  });
});
