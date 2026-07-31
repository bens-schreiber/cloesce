import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { OptionService } from "../fixtures/options/client";
import config from "../fixtures/options/cloesce.jsonc" with { type: "jsonc" };

const workersUrl = config.workers_url!;

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/options", workersUrl);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("option<T> query parameters", () => {
  it("binds omitted query params to null instead of 400ing", async () => {
    // The plain-HTTP shape every non-Cloesce client sends: nothing supplied.
    const res = await fetch(`${workersUrl}/OptionService/search`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("null|null");
  });

  it("accepts a partially supplied query string", async () => {
    // `curl`-shaped: supply one filter, omit the rest.
    const res = await fetch(`${workersUrl}/OptionService/search?limit=20`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("null|number:20");
  });

  it('treats a literal "null" query value as the 4-character string', async () => {
    // Absence is no longer encoded in-band, so "null" is ordinary data.
    const res = await fetch(`${workersUrl}/OptionService/search?tag=null`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("string:null|null");
  });

  it("still 400s when a non-nullable param is missing, naming it", async () => {
    const res = await fetch(`${workersUrl}/OptionService/find?tag=x`);
    expect(res.status).toBe(400);
    const body = await res.text();
    expect(body).toContain("slug");
    expect(body).not.toContain("tag");
  });

  it("omits null params from the query string in the generated client", async () => {
    const res = await OptionService.search(null, null);
    expectHttpResult(res, "Expected omitted optionals to bind to null");
    expect(res.data).toEqual("null|null");
  });

  it('round-trips a literal "null" string through the generated client', async () => {
    const res = await OptionService.search("null", 5);
    expectHttpResult(res, "Expected literal 'null' to survive as a string");
    expect(res.data).toEqual("string:null|number:5");
  });
});

describe("option<T> body parameters", () => {
  it("binds an omitted body key to null", async () => {
    const res = await fetch(`${workersUrl}/OptionService/update`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ bio: "hello" }),
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("string:hello|null");
  });

  it("accepts an explicit JSON null", async () => {
    const res = await fetch(`${workersUrl}/OptionService/update`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ bio: "hi", image: null }),
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("string:hi|null");
  });

  it("accepts a body with every optional key omitted", async () => {
    const res = await fetch(`${workersUrl}/OptionService/update`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("null|null");
  });

  it('keeps the literal "null" string distinct from null in a JSON body', async () => {
    const res = await fetch(`${workersUrl}/OptionService/update`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ bio: "null", image: null }),
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("string:null|null");
  });
});

describe("option<T> header parameters", () => {
  it("binds an absent optional header to null", async () => {
    const res = await fetch(`${workersUrl}/OptionService/whoami`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("null");
  });

  it("reads the optional header when present", async () => {
    const res = await fetch(`${workersUrl}/OptionService/whoami`, {
      headers: { "X-Tenant": "vox" },
    });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual("string:vox");
  });
});
