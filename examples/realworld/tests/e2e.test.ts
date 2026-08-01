import { beforeAll, describe, expect, it } from "vitest";
import { anon, as, expectOk } from "./setup.js";
import { Article, Comment, Tag, User, UserDto } from "@cloesce/client.js";

/**
 * An end-to-end walk through Conduit, over HTTP through the generated client.
 *
 * Every client method the schema produces is exercised here, in the order a real reader turned
 * writer would hit them. The suite shares one database, so the steps build on each other.
 */

// Two readers and a writer, filled in by the first block.
let jane: UserDto;
let bob: UserDto;
let carol: UserDto;
let slug: string;
let commentId: number;

describe("Signing up and signing in", () => {
  it("registers three users", async () => {
    const janeRes = await User.register("jane", "jane@conduit.dev", "password123", anon);
    jane = expectOk(janeRes);
    expect(jane.username).toBe("jane");
    expect(jane.bio).toBe("");
    expect(jane.image).toBe(null);
    expect(jane.token).toBeTypeOf("string");

    bob = expectOk(await User.register("bob", "bob@conduit.dev", "password123", anon));
    carol = expectOk(await User.register("carol", "carol@conduit.dev", "password123", anon));
  });

  it("rejects a registration that fails schema validation", async () => {
    // `email` carries a [regex] validator and `password` a [minlen 8].
    expect((await User.register("mallory", "not-an-email", "password123", anon)).status).toBe(400);
    expect((await User.register("mallory", "mallory@conduit.dev", "short", anon)).status).toBe(400);
  });

  it("rejects a duplicate username", async () => {
    const res = await User.register("jane", "other@conduit.dev", "password123", anon);
    expect(res.status).toBe(422);
  });

  it("logs in and hands back a usable token", async () => {
    const res = await User.login("jane@conduit.dev", "password123", anon);
    expect(expectOk(res).username).toBe("jane");

    // The token from a login works the same as the one from a registration.
    expect(expectOk(await User.current(as(expectOk(res).token))).username).toBe("jane");
  });

  it("refuses a bad password", async () => {
    expect((await User.login("jane@conduit.dev", "wrong-password", anon)).status).toBe(401);
  });

  it("gates the current user behind a token", async () => {
    expect((await User.current(anon)).status).toBe(401);
    expect(expectOk(await User.current(as(jane.token))).email).toBe("jane@conduit.dev");
  });

  it("updates the signed-in user", async () => {
    const res = await User.update({ bio: "Dragon trainer." }, as(jane.token));
    expect(expectOk(res).bio).toBe("Dragon trainer.");
    expect(expectOk(res).username).toBe("jane");

    // ...and it stuck.
    expect(expectOk(await User.current(as(jane.token))).bio).toBe("Dragon trainer.");
  });
});

describe("Profiles and following", () => {
  it("serves a public profile", async () => {
    const res = await User.profile("jane", anon);
    expect(expectOk(res).username).toBe("jane");
    expect(expectOk(res).bio).toBe("Dragon trainer.");
    expect(expectOk(res).following).toBe(false);
  });

  it("404s an unknown profile", async () => {
    expect((await User.profile("nobody", anon)).status).toBe(404);
  });

  it("follows and reflects it back on the profile", async () => {
    expect(expectOk(await User.follow("jane", as(bob.token))).following).toBe(true);
    expect(expectOk(await User.profile("jane", as(bob.token))).following).toBe(true);

    // Following is per-caller and not symmetric.
    expect(expectOk(await User.profile("jane", as(carol.token))).following).toBe(false);
    expect(expectOk(await User.profile("bob", as(jane.token))).following).toBe(false);
  });

  it("unfollows", async () => {
    expectOk(await User.follow("jane", as(carol.token)));
    expect(expectOk(await User.unfollow("jane", as(carol.token))).following).toBe(false);
    expect(expectOk(await User.profile("jane", as(carol.token))).following).toBe(false);
  });

  it("requires a token to follow", async () => {
    expect((await User.follow("jane", anon)).status).toBe(401);
    expect((await User.unfollow("jane", anon)).status).toBe(401);
  });
});

describe("Writing articles", () => {
  it("publishes an article with tags", async () => {
    const res = await Article.create(
      "How to Train Your Dragon",
      "Ever wonder how?",
      "It takes a Jacobian.",
      ["dragons", "training"],
      as(jane.token),
    );

    const created = expectOk(res);
    expect(created.title).toBe("How to Train Your Dragon");
    expect(created.slug).toBeTypeOf("string");
    expect(created.createdAt).toBeInstanceOf(Date);
    slug = created.slug;
  });

  it("requires a token to publish", async () => {
    expect((await Article.create("Anon", "d", "b", [], anon)).status).toBe(401);
  });

  it("reads the article back by slug, hydrated", async () => {
    const got = expectOk(await Article.$get(slug, anon));

    expect(got.title).toBe("How to Train Your Dragon");
    expect(got.body).toBe("It takes a Jacobian.");
    expect(got.tags.map((t) => t.tag?.name).sort()).toEqual(["dragons", "training"]);
    expect(got.comments).toEqual([]);
    expect(got.favoritedBy).toEqual([]);
  });

  it("404s an unknown slug", async () => {
    expect((await Article.$get("no-such-article", anon)).status).toBe(404);
  });

  it("edits an article through its instance method", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const edited = expectOk(await article.update({ body: "It takes a Hessian." }, as(jane.token)));

    expect(edited.body).toBe("It takes a Hessian.");
    expect(edited.title).toBe("How to Train Your Dragon");
    expect(expectOk(await Article.$get(slug, anon)).body).toBe("It takes a Hessian.");
  });

  it("will not let another user edit it", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await article.update({ body: "vandalized" }, as(bob.token))).status).toBe(403);
  });

  it("lists the tags that have been used", async () => {
    const tags = expectOk(await Tag.$list(0, 100, anon));
    expect(tags.map((t) => t.name)).toEqual(expect.arrayContaining(["dragons", "training"]));
  });
});

describe("Browsing the article list", () => {
  beforeAll(async () => {
    await Article.create("Bob on Dragons", "d", "b", ["dragons"], as(bob.token));
    await Article.create("Bob on Cats", "d", "b", ["cats"], as(bob.token));
  });

  it("lists everything with no filter", async () => {
    const all = expectOk(await Article.$list(null, null, null, null, null, anon));
    expect(all.length).toBeGreaterThanOrEqual(3);
    expect(all[0]).toBeInstanceOf(Article);
  });

  it("filters by tag", async () => {
    const dragons = expectOk(await Article.$list("dragons", null, null, null, null, anon));
    expect(dragons.map((a) => a.title).sort()).toEqual([
      "Bob on Dragons",
      "How to Train Your Dragon",
    ]);
  });

  it("filters by author", async () => {
    const byBob = expectOk(await Article.$list(null, "bob", null, null, null, anon));
    expect(byBob.map((a) => a.title).sort()).toEqual(["Bob on Cats", "Bob on Dragons"]);
  });

  it("paginates", async () => {
    const firstTwo = expectOk(await Article.$list(null, null, null, 2, 0, anon));
    const rest = expectOk(await Article.$list(null, null, null, 2, 2, anon));

    expect(firstTwo).toHaveLength(2);
    expect(rest.length).toBeGreaterThanOrEqual(1);
    // Pages do not overlap.
    const ids = new Set(firstTwo.map((a) => a.id));
    expect(rest.every((a) => !ids.has(a.id))).toBe(true);
  });

  it("serves a personal feed of followed authors only", async () => {
    // bob follows jane, and only jane.
    const feed = expectOk(await User.feed(null, null, as(bob.token)));
    expect(feed.map((a) => a.title)).toEqual(["How to Train Your Dragon"]);

    // carol follows nobody.
    expect(expectOk(await User.feed(null, null, as(carol.token)))).toEqual([]);
  });

  it("requires a token for the feed", async () => {
    expect((await User.feed(null, null, anon)).status).toBe(401);
  });
});

describe("Favoriting", () => {
  it("favorites an article, counting it in the article's Durable Object", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expectOk(await article.favorite(as(bob.token)));

    const got = expectOk(await Article.$get(slug, anon));
    expect(got.favoriteCount).toBe(1);
    expect(got.favoritedBy).toHaveLength(1);
  });

  it("counts a second reader, and is idempotent per reader", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expectOk(await article.favorite(as(carol.token)));
    expectOk(await article.favorite(as(carol.token)));

    expect(expectOk(await Article.$get(slug, anon)).favoriteCount).toBe(2);
  });

  it("filters the list by who favorited", async () => {
    const favorited = expectOk(await Article.$list(null, null, "carol", null, null, anon));
    expect(favorited.map((a) => a.slug)).toEqual([slug]);
  });

  it("unfavorites, bringing the count back down", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expectOk(await article.unfavorite(as(carol.token)));

    expect(expectOk(await Article.$get(slug, anon)).favoriteCount).toBe(1);

    // Unfavoriting again is a no-op rather than a negative count.
    expectOk(await article.unfavorite(as(carol.token)));
    expect(expectOk(await Article.$get(slug, anon)).favoriteCount).toBe(1);
  });

  it("requires a token", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await article.favorite(anon)).status).toBe(401);
    expect((await article.unfavorite(anon)).status).toBe(401);
  });
});

describe("Commenting", () => {
  it("posts a comment", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const posted = expectOk(await Comment.create(article.id, "Great read!", as(bob.token)));

    expect(posted.body).toBe("Great read!");
    expect(posted.createdAt).toBeInstanceOf(Date);
    commentId = posted.id;
  });

  it("requires a token to comment", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await Comment.create(article.id, "spam", anon)).status).toBe(401);
  });

  it("lists the comments on an article", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const listed = expectOk(await Comment.$list(article.id, anon));

    expect(listed.map((c) => c.body)).toEqual(["Great read!"]);
    expect(listed[0].id).toBe(commentId);
  });

  it("hangs the comments off the article itself", async () => {
    const got = expectOk(await Article.$get(slug, anon));
    expect(got.comments.map((c) => c.body)).toEqual(["Great read!"]);
  });

  it("edits a comment in place", async () => {
    const posted = expectOk(
      await Comment.$list(expectOk(await Article.$get(slug, anon)).id, anon),
    )[0];
    const edited = expectOk(await posted.update({ body: "Great read, actually!" }, as(bob.token)));

    expect(edited.id).toBe(commentId);
    expect(edited.body).toBe("Great read, actually!");
  });

  it("will not let another user edit or delete it", async () => {
    const posted = expectOk(
      await Comment.$list(expectOk(await Article.$get(slug, anon)).id, anon),
    )[0];
    expect((await posted.update({ body: "nope" }, as(carol.token))).status).toBe(403);
    expect((await posted.del(as(carol.token))).status).toBe(403);
  });

  it("deletes a comment", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const posted = expectOk(await Comment.$list(article.id, anon))[0];

    expectOk(await posted.del(as(bob.token)));
    expect(expectOk(await Comment.$list(article.id, anon))).toEqual([]);
  });
});

describe("Deleting an article", () => {
  it("will not let another user delete it", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await article.del(as(bob.token))).status).toBe(403);
  });

  it("deletes it, along with its tags, favorites and comments", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    await Comment.create(article.id, "last words", as(bob.token));

    expectOk(await article.del(as(jane.token)));

    expect((await Article.$get(slug, anon)).status).toBe(404);
    expect(expectOk(await Comment.$list(article.id, anon))).toEqual([]);
    expect(expectOk(await Article.$list(null, "jane", null, null, null, anon))).toEqual([]);
  });
});
