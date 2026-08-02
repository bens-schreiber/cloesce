import { beforeAll, describe, expect, it } from "vitest";
import { anon, as, expectOk } from "./setup.js";
import { Article, Comment, Tag, User, UserDto } from "@cloesce/client.js";

/**
 * An end-to-end walk through Conduit, over HTTP through the generated client.
 *
 * Every client method the schema produces is exercised here, in the order a real reader turned
 * writer would hit them. The suite shares one database, so the steps build on each other.
 */

const vex = {
  dto: undefined as unknown as UserDto,
  profile: { username: "vex", email: "vex@vasselheim.dev", bio: "", image: null },
};
const grog = {
  dto: undefined as unknown as UserDto,
  profile: { username: "grog", email: "grog@vasselheim.dev", bio: "", image: null },
};
const percy = {
  dto: undefined as unknown as UserDto,
  profile: { username: "percy", email: "percy@whitestone.dev", bio: "", image: null },
};
const PASSWORD = "password123";

beforeAll(async () => {
  vex.dto = expectOk(await User.register(vex.profile.username, vex.profile.email, PASSWORD, anon));
  expect(vex.dto).toMatchObject(vex.profile);
  expect(vex.dto.token).toBeTypeOf("string");

  grog.dto = expectOk(
    await User.register(grog.profile.username, grog.profile.email, PASSWORD, anon),
  );
  expect(grog.dto).toMatchObject(grog.profile);

  percy.dto = expectOk(
    await User.register(percy.profile.username, percy.profile.email, PASSWORD, anon),
  );
  expect(percy.dto).toMatchObject(percy.profile);
});

describe("Signing up and signing in", () => {
  it("rejects a registration that fails schema validation", async () => {
    // `email` carries a [regex] validator and `password` a [minlen 8].
    expect((await User.register("scanlan", "not-an-email", PASSWORD, anon)).status).toBe(400);
    expect((await User.register("scanlan", "scanlan@vasselheim.dev", "short", anon)).status).toBe(
      400,
    );
  });

  it("rejects a duplicate username", async () => {
    const res = await User.register(vex.profile.username, "other@vasselheim.dev", PASSWORD, anon);
    expect(res.status).toBe(422);
  });

  it("logs in and hands back a usable token", async () => {
    const res = await User.login(vex.profile.email, PASSWORD, anon);
    expect(expectOk(res).username).toBe(vex.profile.username);

    // The token from a login works the same as the one from a registration.
    expect(expectOk(await User.current(as(expectOk(res).token))).username).toBe(
      vex.profile.username,
    );
  });

  it("refuses a bad password", async () => {
    expect((await User.login(vex.profile.email, "wrong-password", anon)).status).toBe(401);
  });

  it("gates the current user behind a token", async () => {
    expect((await User.current(anon)).status).toBe(401);
    expect(expectOk(await User.current(as(vex.dto.token))).email).toBe(vex.profile.email);
  });

  it("updates the signed-in user", async () => {
    const newBio = "Ranger of the Myriad Tree.";
    const res = await User.update({ bio: newBio }, as(vex.dto.token));
    expect(expectOk(res).bio).toBe(newBio);
    expect(expectOk(res).username).toBe(vex.profile.username);

    // ...and it stuck.
    expect(expectOk(await User.current(as(vex.dto.token))).bio).toBe(newBio);
  });
});

describe("Avatars", () => {
  it("404s a user with no avatar", async () => {
    expect((await User.downloadAvatar(vex.profile.username, anon)).status).toBe(404);
  });

  it("uploads and downloads an avatar", async () => {
    const bytes = new Uint8Array([1, 2, 3, 4, 5]);
    expectOk(await User.uploadAvatar(bytes, as(vex.dto.token)));

    const res = expectOk(await User.downloadAvatar(vex.profile.username, anon));
    const got = new Uint8Array(await res.arrayBuffer());
    expect(got).toEqual(bytes);
  });

  it("requires a token to upload", async () => {
    expect((await User.uploadAvatar(new Uint8Array([1]), anon)).status).toBe(401);
  });

  it("404s an unknown username", async () => {
    expect((await User.downloadAvatar("nobody", anon)).status).toBe(404);
  });
});

describe("Profiles and following", () => {
  it("serves a public profile", async () => {
    const res = await User.profile(grog.profile.username, anon);
    expect(expectOk(res)).toMatchObject({
      username: grog.profile.username,
      bio: grog.profile.bio,
      following: false,
    });
  });

  it("404s an unknown profile", async () => {
    expect((await User.profile("nobody", anon)).status).toBe(404);
  });

  it("follows and reflects it back on the profile", async () => {
    expect(expectOk(await User.follow(vex.profile.username, as(grog.dto.token))).following).toBe(
      true,
    );
    expect(expectOk(await User.profile(vex.profile.username, as(grog.dto.token))).following).toBe(
      true,
    );

    // Following is per-caller and not symmetric.
    expect(expectOk(await User.profile(vex.profile.username, as(percy.dto.token))).following).toBe(
      false,
    );
    expect(expectOk(await User.profile(grog.profile.username, as(vex.dto.token))).following).toBe(
      false,
    );
  });

  it("unfollows", async () => {
    expectOk(await User.follow(vex.profile.username, as(percy.dto.token)));
    expect(expectOk(await User.unfollow(vex.profile.username, as(percy.dto.token))).following).toBe(
      false,
    );
    expect(expectOk(await User.profile(vex.profile.username, as(percy.dto.token))).following).toBe(
      false,
    );
  });

  it("requires a token to follow", async () => {
    expect((await User.follow(vex.profile.username, anon)).status).toBe(401);
    expect((await User.unfollow(vex.profile.username, anon)).status).toBe(401);
  });
});

const DRAGON_ARTICLE_TITLE = "How to Track a White Dragon";
const DRAGON_ARTICLE_BODY = "It takes a Trinket.";
const EDITED_ARTICLE_BODY = "It takes the Deathwalker's Ward.";
const DRAGONS_TAG = "dragons";
const TRACKING_TAG = "tracking";
let slug: string;

describe("Writing articles", () => {
  it("publishes an article with tags", async () => {
    const res = await Article.create(
      DRAGON_ARTICLE_TITLE,
      "Ever wonder how?",
      DRAGON_ARTICLE_BODY,
      [DRAGONS_TAG, TRACKING_TAG],
      as(vex.dto.token),
    );

    const created = expectOk(res);
    expect(created.title).toBe(DRAGON_ARTICLE_TITLE);
    expect(created.slug).toBeTypeOf("string");
    expect(created.createdAt).toBeInstanceOf(Date);
    slug = created.slug;
  });

  it("requires a token to publish", async () => {
    expect((await Article.create("Anon", "d", "b", [], anon)).status).toBe(401);
  });

  it("reads the article back by slug, hydrated", async () => {
    const got = expectOk(await Article.$get(slug, anon));

    expect(got.title).toBe(DRAGON_ARTICLE_TITLE);
    expect(got.body).toBe(DRAGON_ARTICLE_BODY);
    expect(got.tags.map((t) => t.tag?.name).sort()).toEqual([DRAGONS_TAG, TRACKING_TAG]);
    expect(got.comments).toEqual([]);
    expect(got.favoritedBy).toEqual([]);
  });

  it("404s an unknown slug", async () => {
    expect((await Article.$get("no-such-article", anon)).status).toBe(404);
  });

  it("edits an article through its instance method", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const edited = expectOk(await article.update({ body: EDITED_ARTICLE_BODY }, as(vex.dto.token)));

    expect(edited.body).toBe(EDITED_ARTICLE_BODY);
    expect(edited.title).toBe(DRAGON_ARTICLE_TITLE);
    expect(expectOk(await Article.$get(slug, anon)).body).toBe(EDITED_ARTICLE_BODY);
  });

  it("will not let another user edit it", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await article.update({ body: "vandalized" }, as(grog.dto.token))).status).toBe(403);
  });

  it("lists the tags that have been used", async () => {
    const tags = expectOk(await Tag.$list(0, 100, anon));
    expect(tags.map((t) => t.name)).toEqual(expect.arrayContaining([DRAGONS_TAG, TRACKING_TAG]));
  });
});

describe("Browsing the article list", () => {
  const grogDragonsTitle = "Grog on Dragons";
  const grogBearsTitle = "Grog on Bears";
  const bearsTag = "bears";

  beforeAll(async () => {
    await Article.create(grogDragonsTitle, "d", "b", [DRAGONS_TAG], as(grog.dto.token));
    await Article.create(grogBearsTitle, "d", "b", [bearsTag], as(grog.dto.token));
  });

  it("lists everything with no filter", async () => {
    const all = expectOk(await Article.$list(null, null, null, null, null, anon));
    expect(all.length).toBeGreaterThanOrEqual(3);
    expect(all[0]).toBeInstanceOf(Article);
  });

  it("filters by tag", async () => {
    const dragons = expectOk(await Article.$list(DRAGONS_TAG, null, null, null, null, anon));
    expect(dragons.map((a) => a.title).sort()).toEqual([grogDragonsTitle, DRAGON_ARTICLE_TITLE]);
  });

  it("filters by author", async () => {
    const byGrog = expectOk(
      await Article.$list(null, grog.profile.username, null, null, null, anon),
    );
    expect(byGrog.map((a) => a.title).sort()).toEqual([grogBearsTitle, grogDragonsTitle]);
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
    // grog follows vex, and only vex.
    const feed = expectOk(await User.feed(null, null, as(grog.dto.token)));
    expect(feed.map((a) => a.title)).toEqual([DRAGON_ARTICLE_TITLE]);

    // percy follows nobody.
    expect(expectOk(await User.feed(null, null, as(percy.dto.token)))).toEqual([]);
  });

  it("requires a token for the feed", async () => {
    expect((await User.feed(null, null, anon)).status).toBe(401);
  });
});

describe("Favoriting", () => {
  it("favorites an article, counting it in the article's Durable Object", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expectOk(await article.favorite(as(grog.dto.token)));

    const got = expectOk(await Article.$get(slug, anon));
    expect(got.favoriteCount).toBe(1);
    expect(got.favoritedBy).toHaveLength(1);
  });

  it("counts a second reader, and is idempotent per reader", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expectOk(await article.favorite(as(percy.dto.token)));
    expectOk(await article.favorite(as(percy.dto.token)));

    expect(expectOk(await Article.$get(slug, anon)).favoriteCount).toBe(2);
  });

  it("filters the list by who favorited", async () => {
    const favorited = expectOk(
      await Article.$list(null, null, percy.profile.username, null, null, anon),
    );
    expect(favorited.map((a) => a.slug)).toEqual([slug]);
  });

  it("unfavorites, bringing the count back down", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expectOk(await article.unfavorite(as(percy.dto.token)));

    expect(expectOk(await Article.$get(slug, anon)).favoriteCount).toBe(1);

    // Unfavoriting again is a no-op rather than a negative count.
    expectOk(await article.unfavorite(as(percy.dto.token)));
    expect(expectOk(await Article.$get(slug, anon)).favoriteCount).toBe(1);
  });

  it("requires a token", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await article.favorite(anon)).status).toBe(401);
    expect((await article.unfavorite(anon)).status).toBe(401);
  });
});

const COMMENT_BODY = "Trinket approves!";
const EDITED_COMMENT_BODY = "Trinket really approves!";

describe("Commenting", () => {
  let commentId: number;
  it("posts a comment", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const posted = expectOk(await Comment.create(article.id, COMMENT_BODY, as(grog.dto.token)));

    expect(posted.body).toBe(COMMENT_BODY);
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

    expect(listed.map((c) => c.body)).toEqual([COMMENT_BODY]);
    expect(listed[0].id).toBe(commentId);
  });

  it("hangs the comments off the article itself", async () => {
    const got = expectOk(await Article.$get(slug, anon));
    expect(got.comments.map((c) => c.body)).toEqual([COMMENT_BODY]);
  });

  it("edits a comment in place", async () => {
    const posted = expectOk(
      await Comment.$list(expectOk(await Article.$get(slug, anon)).id, anon),
    )[0];
    const edited = expectOk(await posted.update({ body: EDITED_COMMENT_BODY }, as(grog.dto.token)));

    expect(edited.id).toBe(commentId);
    expect(edited.body).toBe(EDITED_COMMENT_BODY);
  });

  it("will not let another user edit or delete it", async () => {
    const posted = expectOk(
      await Comment.$list(expectOk(await Article.$get(slug, anon)).id, anon),
    )[0];
    expect((await posted.update({ body: "nope" }, as(percy.dto.token))).status).toBe(403);
    expect((await posted.del(as(percy.dto.token))).status).toBe(403);
  });

  it("deletes a comment", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    const posted = expectOk(await Comment.$list(article.id, anon))[0];

    expectOk(await posted.del(as(grog.dto.token)));
    expect(expectOk(await Comment.$list(article.id, anon))).toEqual([]);
  });
});

describe("Deleting an article", () => {
  it("will not let another user delete it", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    expect((await article.del(as(grog.dto.token))).status).toBe(403);
  });

  it("deletes it, along with its tags, favorites and comments", async () => {
    const article = expectOk(await Article.$get(slug, anon));
    await Comment.create(article.id, "last words", as(grog.dto.token));

    expectOk(await article.del(as(vex.dto.token)));

    expect((await Article.$get(slug, anon)).status).toBe(404);
    expect(expectOk(await Comment.$list(article.id, anon))).toEqual([]);
    expect(
      expectOk(await Article.$list(null, vex.profile.username, null, null, null, anon)),
    ).toEqual([]);
  });
});
