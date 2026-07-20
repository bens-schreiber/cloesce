import { describe, expect, it } from "vitest";

import { inPost, inUser, onWorker } from "./setup.js";
import { SubReddit } from "../src/api/sub.js";
import { Post, Comment } from "../src/api/post.js";
import { User } from "../src/api/user.js";

function newSub(as: string, title = "r/dogs", description = "woof") {
  return onWorker(as, (e) => SubReddit.create(e, title, description), ["SubRedditDb", "UserDo"]);
}

function newPost(as: string, subId: number, title: string, content: string) {
  return onWorker(as, (e) => Post.create(e, subId, title, content), [
    "SubRedditDb",
    "PostDo",
    "UserDo",
  ]);
}

function newComment(as: string, doId: string, content: string) {
  return inPost(doId, as, (e) => Comment.create(e, doId, content), ["PostDo", "UserDo"]);
}

function feedOf(as: string, sub: any) {
  return onWorker(as, (e) => SubReddit.feed(sub, e), ["SubRedditDb", "PostDo", "UserDo"]);
}

describe("Auth", () => {
  it("login claims a username and returns a token + user", async () => {
    const res = await inUser("alice", null, (e) => User.login(e, "alice"), ["Sessions"]);
    expect(res.token).toBeTypeOf("string");
    expect(res.user.name).toBe("alice");
  });

  it("posting while anonymous is 401", async () => {
    const sub = await newSub("alice");
    const res = await newPost(null as any, sub.id, "anon", "nope");
    expect(res.status).toBe(401);
  });
});

describe("Subreddits", () => {
  it("a logged-in user can create one (D1 assigns its id)", async () => {
    const sub = await newSub("alice");
    expect(sub.id).toBeTypeOf("number");
    expect(sub.title).toBe("r/dogs");
    expect(sub.lastPostId).toBe(0);
  });

  it("created subreddits appear in the global listing", async () => {
    const sub = await newSub("alice", "r/listed", "");
    const dir = await onWorker("alice", (e) => SubReddit.Default.list(e, 0, 100), ["SubRedditDb"]);
    expect(dir.data!.map((s: any) => s.id)).toContain(sub.id);
  });
});

describe("Posts, comments, and the feed", () => {
  it("create a post, comment on it, then read it back with its comments", async () => {
    const sub = await newSub("alice");
    const post = await newPost("alice", sub.id, "Cats", "Discuss.");
    expect(post.meta.authorName).toBe("alice");
    expect(post.meta.upvotes).toBe(0);

    await newComment("alice", post.doId, "agreed!");

    const view = (
      await inPost(post.doId, "alice", (e) => Post.Default.get(e, post.doId), ["PostDo"])
    ).data!;
    expect(view.meta.title).toBe("Cats");
    expect(view.comments.map((c: any) => c.content)).toContain("agreed!");
  });

  it("each post gets its own DO, so comments do not leak between posts", async () => {
    const sub = await newSub("alice");
    const a = await newPost("alice", sub.id, "A", "a");
    const b = await newPost("alice", sub.id, "B", "b");
    expect(a.doId).not.toBe(b.doId);

    await newComment("alice", a.doId, "only on A");

    const viewB = (await inPost(b.doId, "alice", (e) => Post.Default.get(e, b.doId), ["PostDo"]))
      .data!;
    expect(viewB.comments).toEqual([]);
  });

  it("the feed hydrates each post out of its own DO, isolated per sub", async () => {
    const sub = await newSub("alice");
    const other = await newSub("alice", "r/empty", "");
    const post = await newPost("alice", sub.id, "in sub", "body");
    await newComment("alice", post.doId, "nice");

    const withPosts = (
      await onWorker("alice", (e) => SubReddit.Default.get(e, sub.id), ["SubRedditDb"])
    ).data!;
    const feed = await feedOf("alice", withPosts);

    expect(feed.map((p: any) => p.doId)).toEqual([post.doId]);
    expect(feed[0].meta.title).toBe("in sub");
    expect(feed[0].comments.map((c: any) => c.content)).toEqual(["nice"]);

    const emptySub = (
      await onWorker("alice", (e) => SubReddit.Default.get(e, other.id), ["SubRedditDb"])
    ).data!;
    expect(await feedOf("alice", emptySub)).toEqual([]);
  });
});

describe("Voting", () => {
  it("up then down nets to zero on a post; up works on a comment", async () => {
    const sub = await newSub("alice");
    const post = await newPost("alice", sub.id, "vote", "body");

    // vote is an instance method: pass the latest model back in each time.
    const up = await inPost(post.doId, "alice", (e) => Post.vote(post, e, 1), ["PostDo"]);
    expect(up.meta.upvotes).toBe(1);
    const down = await inPost(post.doId, "alice", (e) => Post.vote(up, e, -1), ["PostDo"]);
    expect(down.meta.upvotes).toBe(0);

    const c = await newComment("alice", post.doId, "hi");
    const cUp = await inPost(post.doId, "alice", (e) => Comment.vote(c, e, 1), [
      "PostDo",
      "UserDo",
    ]);
    expect(cUp.upvotes).toBe(1);
  });
});

describe("Authorship", () => {
  it("creating things records them in the author's own DO", async () => {
    await inUser("dana", null, (e) => User.login(e, "dana"), ["Sessions"]);
    const sub = await newSub("dana", "r/d", "d");
    const post = await newPost("dana", sub.id, "p", "b");
    await newComment("dana", post.doId, "c");

    const user = (
      await inUser("dana", "dana", (e) => User.Default.get(e, "dana"), [
        "SubRedditDb",
        "PostDo",
        "UserDo",
      ])
    ).data!;
    expect(user.authoredSubReddits.map((s: any) => s.subRedditId)).toContain(sub.id);
    expect(user.authoredPosts.map((p: any) => p.postId)).toContain(post.doId);
    expect(user.authoredComments.length).toBe(1);
  });
});
