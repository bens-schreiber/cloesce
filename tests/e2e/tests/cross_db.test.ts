import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { expectHttpResult, startWrangler } from "../src/setup";
import { Author, Book } from "../fixtures/cross_db/client";
import config from "../fixtures/cross_db/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/cross_db", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Cross-database query planning", () => {
  let tolkien: Author;
  let austen: Author;

  it("$save_Deep persists a D1 root, its DO-sharded children, and their KV leaves in one call", async () => {
    const res = await Author.$save_Deep({
      name: "tolkien",
      books: [
        { title: "The Hobbit", blurb: { raw: "there and back again" } },
        { title: "The Silmarillion", blurb: { raw: "elves, mostly" } },
      ],
    });
    expectHttpResult(res, "$save_Deep should be OK");
    tolkien = res.data!;

    expect(tolkien.id).toBeTypeOf("number");
    expect(tolkien.books.length).toBe(2);
    expect(tolkien.books.every((b) => b.authorId === tolkien.id)).toBe(true);
    expect(tolkien.books.map((b) => b.title).sort()).toEqual(["The Hobbit", "The Silmarillion"]);
  });

  it("$get_Deep hydrates D1 -> DO -> KV in a single select plan", async () => {
    const res = await Author.$get_Deep(tolkien.id);
    expectHttpResult(res, "$get_Deep should be OK");

    const author = res.data!;
    expect(author.name).toBe("tolkien");
    expect(author.books.length).toBe(2);

    const hobbit = author.books.find((b) => b.title === "The Hobbit")!;
    expect(hobbit.blurb.value).toBe("there and back again");
    const silmarillion = author.books.find((b) => b.title === "The Silmarillion")!;
    expect(silmarillion.blurb.value).toBe("elves, mostly");
  });

  it("books are isolated per author shard", async () => {
    const res = await Author.$save_Deep({
      name: "austen",
      books: [{ title: "Persuasion", blurb: { raw: "second chances" } }],
    });
    expectHttpResult(res, "$save_Deep should be OK");
    austen = res.data!;
    expect(austen.id).not.toBe(tolkien.id);

    const got = await Author.$get_Deep(austen.id);
    expectHttpResult(got, "$get_Deep should be OK");
    expect(got.data!.books.length).toBe(1);
    expect(got.data!.books[0].title).toBe("Persuasion");
  });

  it("$list_Deep fans out to every author's DO shard", async () => {
    const res = await Author.$list_Deep(0, 100);
    expectHttpResult(res, "$list_Deep should be OK");

    expect(res.data!.length).toBe(2);
    const byName = Object.fromEntries(res.data!.map((a) => [a.name, a]));
    expect(byName["tolkien"].books.length).toBe(2);
    expect(byName["austen"].books.length).toBe(1);
    expect(byName["austen"].books[0].blurb.value).toBe("second chances");
  });

  it("$get_WithAuthor hydrates a DO root's back-reference into D1 plus its KV leaf", async () => {
    const persuasionId = austen.books[0].id;
    const res = await Book.$get_WithAuthor(austen.id, persuasionId);
    expectHttpResult(res, "$get_WithAuthor should be OK");

    const book = res.data!;
    expect(book.title).toBe("Persuasion");
    expect(book.author!.name).toBe("austen");
    expect(book.blurb.value).toBe("second chances");
  });

  it("$save on the DO shard directly is visible from the D1 root's include", async () => {
    const saved = await Book.$save(austen.id, { title: "Emma" });
    expectHttpResult(saved, "$save should be OK");
    expect(saved.data!.authorId).toBe(austen.id);

    const got = await Author.$get_Deep(austen.id);
    expectHttpResult(got, "$get_Deep should be OK");
    expect(got.data!.books.map((b) => b.title).sort()).toEqual(["Emma", "Persuasion"]);
  });

  it("plain $get returns the D1 root without crossing databases", async () => {
    const res = await Author.$get(tolkien.id);
    expectHttpResult(res, "$get should be OK");
    expect(res.data!.name).toBe("tolkien");
  });
});
