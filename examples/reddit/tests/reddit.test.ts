import { describe, expect, it } from "vitest";
import { app } from "./setup.js";

describe("Auth", () => {
  it("login claims a username and returns a token + user", async () => {
    const res = await app().env.UserDo.user.login("alice");
    expect(res.data?.token).toBeTypeOf("string");
    expect(res.data?.user.name).toBe("alice");
  });

  it("posting while anonymous is 401", async () => {
    const sub = await app("user").env.SubRedditDb.subReddit.create("r/anon", "anon");
    const res = await app().env.PostDo.post.create(sub.data!.id, "anon", "nope");
    expect(res.status).toBe(401);
  });
});

describe("Subreddits", () => {
  it("a logged-in user can create one (D1 assigns its id)", async () => {
    const env = app("alice").env;
    const sub = await env.SubRedditDb.subReddit.create("r/dogs", "doggos");
    expect(sub.data?.id).toBeTypeOf("number");
    expect(sub.data?.title).toBe("r/dogs");
  });

  it("created subreddits appear in the global listing", async () => {
    const subs = app("alice").env.SubRedditDb.subReddit;

    const sub = await subs.create("r/cats", "catto");
    const dir = await subs.list(0, 100);
    expect(dir.data!.map((s: any) => s.id)).toContain(sub.data?.id);
  });
});

describe("Posts, comments, and the feed", () => {
  it("create a post, comment on it, then read it back with its comments", async () => {
    const env = app("alice").env;
    const subs = env.SubRedditDb.subReddit;
    const posts = env.PostDo.post;
    const comments = env.PostDo.comment;

    const sub = await subs.create("r/test", "test");
    const post = (await posts.create(sub.data!.id, "Cats", "meow")).data!;

    expect(post.meta.authorName).toBe("alice");
    expect(post.meta.upvotes).toBe(0);

    await comments.create(post.doId, "agreed!");

    const view = (await env.PostDo.post.get(post.doId)).data!;
    expect(view.meta.title).toBe("Cats");
    expect(view.comments.map((c: any) => c.content)).toContain("agreed!");
  });

  it("each post gets its own DO, so comments do not leak between posts", async () => {
    const env = app("alice").env;
    const subs = env.SubRedditDb.subReddit;
    const posts = env.PostDo.post;
    const comments = env.PostDo.comment;

    const sub = await subs.create("r/test", "test");
    const a = (await posts.create(sub.data!.id, "A", "a")).data!;
    const b = (await posts.create(sub.data!.id, "B", "b")).data!;
    expect(a.doId).not.toBe(b.doId);

    await comments.create(a.doId, "only on A");

    const viewB = (await env.PostDo.post.get(b.doId)).data!;
    expect(viewB.comments).toEqual([]);
  });

  it("the feed hydrates each post out of its own DO, isolated per sub", async () => {
    const env = app("alice").env;
    const subs = env.SubRedditDb.subReddit;
    const posts = env.PostDo.post;
    const comments = env.PostDo.comment;

    const sub = await subs.create("r/feed", "feed");
    const other = await subs.create("r/empty", "empty");
    const post = (await posts.create(sub.data!.id, "in sub", "body")).data!;
    await comments.create(post.doId, "nice");

    const withPosts = (await subs.get(sub.data!.id)).data!;
    const feed = (await subs.feed(withPosts)).data!;

    expect(feed.map((p: any) => p.doId)).toEqual([post.doId]);
    expect(feed[0].meta.title).toBe("in sub");
    expect(feed[0].comments.map((c: any) => c.content)).toEqual(["nice"]);

    const emptySub = (await subs.get(other.data!.id)).data!;
    expect((await subs.feed(emptySub)).data).toEqual([]);
  });
});

describe("Voting", () => {
  it("up then down nets to zero on a post; up works on a comment", async () => {
    const env = app("alice").env;
    const subs = env.SubRedditDb.subReddit;
    const posts = env.PostDo.post;
    const comments = env.PostDo.comment;

    const sub = await subs.create("r/vote", "vote");
    const post = (await posts.create(sub.data!.id, "vote", "body")).data!;

    const up = (await posts.vote(post, 1)).data!;
    expect(up.meta.upvotes).toBe(1);
    const down = (await posts.vote(up, -1)).data!;
    expect(down.meta.upvotes).toBe(0);

    const c = (await comments.create(post.doId, "only on A")).data!;
    const cUp = (await comments.vote(c, 1)).data!;
    expect(cUp.upvotes).toBe(1);
  });
});

describe("Authorship", () => {
  it("creating things records them in the author's own DO", async () => {
    const env = app("dana").env;
    const users = env.UserDo.user;
    const subs = env.SubRedditDb.subReddit;
    const posts = env.PostDo.post;
    const comments = env.PostDo.comment;

    await users.login("dana");
    const sub = (await subs.create("r/dana", "dana")).data!;
    const post = (await posts.create(sub.id, "Cats", "meow")).data!;
    await comments.create(post.doId, "agreed!");

    const user = (await users.get("dana")).data!;
    expect(user.authoredSubReddits.map((s: any) => s.subRedditId)).toContain(sub.id);
    expect(user.authoredPosts.map((p: any) => p.postId)).toContain(post.doId);
    expect(user.authoredComments.length).toBe(1);
  });
});
