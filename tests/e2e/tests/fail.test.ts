import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { FailModel, UnregisteredService } from "../fixtures/fail/client";
import config from "../fixtures/fail/cloesce.jsonc" with { type: "jsonc" };

const BASE = config.workers_url!;

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  stopWrangler = await startWrangler("./fixtures/fail", BASE);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

let saved: FailModel;

describe("Cloesce Router fail cases", () => {
  it("setup: save a FailModel for instantiated-method tests", async () => {
    const res = await FailModel.$save({ id: 1, name: "alpha" });
    expect(res.ok, withRes("save should succeed", res)).toBe(true);
    saved = res.data!;
  });

  describe("route matching", () => {
    it("UnknownPrefix: request outside of /api/ prefix -> 404", async () => {
      const url = BASE.replace("/api", "/notapi") + "/FailModel/$list";
      const res = await fetch(url);
      expect(res.status).toBe(404);
    });

    it("UnknownPrefix: /api/ with too few segments -> 404", async () => {
      const res = await fetch(`${BASE}/FailModel`);
      expect(res.status).toBe(404);
    });

    it("UnknownRoute: namespace does not exist -> 404", async () => {
      const res = await fetch(`${BASE}/NoSuchModel/$list`);
      expect(res.status).toBe(404);
    });

    it("UnknownRoute: model exists but method does not -> 404", async () => {
      const res = await fetch(`${BASE}/FailModel/notARealMethod`);
      expect(res.status).toBe(404);
    });

    it("UnknownRoute: service exists but method does not -> 404", async () => {
      const res = await fetch(`${BASE}/UnregisteredService/notARealMethod`);
      expect(res.status).toBe(404);
    });

    it("UnknownRoute: instantiated method called as static (wrong segment count) -> 404", async () => {
      const res = await fetch(`${BASE}/FailModel/throwingMethod`, { method: "POST" });
      expect(res.status).toBe(404);
    });

    it("UnknownRoute: service method called with extra segments -> 404", async () => {
      const res = await fetch(`${BASE}/UnregisteredService/extra/unregistered`);
      expect(res.status).toBe(404);
    });

    it("UnmatchedHttpVerb: GET against a POST method -> 404", async () => {
      const res = await fetch(`${BASE}/FailModel/${saved.id}/throwingMethod`);
      expect(res.status).toBe(404);
    });

    it("UnmatchedHttpVerb: POST against a GET method -> 404", async () => {
      const res = await fetch(`${BASE}/UnregisteredService/unregistered`, { method: "POST" });
      expect(res.status).toBe(404);
    });

    it("NotImplemented: service method whose impl was never registered -> 501", async () => {
      const res = await UnregisteredService.unregistered();
      expect(res.ok, withRes("Expected 501", res)).toBe(false);
      expect(res.status).toBe(501);
    });
  });

  describe("path parameter validation", () => {
    it("non-int key segment for an int primary key -> 400", async () => {
      const res = await fetch(`${BASE}/FailModel/not-an-int/throwingMethod`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });
      expect(res.status).toBe(400);
    });

    it("ModelNotFound: instantiated method on non-existent id -> 404", async () => {
      const res = await fetch(`${BASE}/FailModel/999999/throwingMethod`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });
      expect(res.status).toBe(404);
    });
  });

  describe("body validation", () => {
    it("RequestMissingBody: POST with no body -> 400", async () => {
      const res = await fetch(`${BASE}/FailModel/${saved.id}/throwingMethod`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
      });
      expect(res.status).toBe(400);
    });

    it("RequestMissingBody: POST with malformed JSON -> 400", async () => {
      const res = await fetch(`${BASE}/FailModel/${saved.id}/throwingMethod`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{not json",
      });
      expect(res.status).toBe(400);
    });

    it("RequestBodyMissingParameters: POST missing required params -> 400", async () => {
      // numericValidators expects 5 fields; send {}.
      const res = await fetch(`${BASE}/FailModel/${saved.id}/numericValidators`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({}),
      });
      expect(res.status).toBe(400);
    });

    it("RequestBodyInvalidParameter: param has wrong primitive type -> 400", async () => {
      const res = await fetch(`${BASE}/FailModel/${saved.id}/numericValidators`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          gtField: "not-a-number",
          gteField: 10,
          ltField: 50,
          lteField: 50,
          stepField: 5,
        }),
      });
      expect(res.status).toBe(400);
    });
  });

  // --------------- ORM validators (numeric) ---------------

  describe("numeric validators", () => {
    const validBase = {
      gtField: 11, // >10
      gteField: 10, // >=10
      ltField: 50, // <100
      lteField: 100, // <=100
      stepField: 5, // multiple of 5
    };

    it("happy path: valid numeric inputs -> 200", async () => {
      const res = await saved.numericValidators(
        validBase.gtField,
        validBase.gteField,
        validBase.ltField,
        validBase.lteField,
        validBase.stepField,
      );
      expect(res.ok, withRes("Expected success", res)).toBe(true);
    });

    it("[gt 10] fails when value equals the bound -> 400", async () => {
      const res = await saved.numericValidators(
        10,
        validBase.gteField,
        validBase.ltField,
        validBase.lteField,
        validBase.stepField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[gte 10] fails when value below the bound -> 400", async () => {
      const res = await saved.numericValidators(
        validBase.gtField,
        9,
        validBase.ltField,
        validBase.lteField,
        validBase.stepField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[lt 100] fails when value equals the bound -> 400", async () => {
      const res = await saved.numericValidators(
        validBase.gtField,
        validBase.gteField,
        100,
        validBase.lteField,
        validBase.stepField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[lte 100] fails when value above the bound -> 400", async () => {
      const res = await saved.numericValidators(
        validBase.gtField,
        validBase.gteField,
        validBase.ltField,
        101,
        validBase.stepField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[step 5] fails when value is not a multiple -> 400", async () => {
      const res = await saved.numericValidators(
        validBase.gtField,
        validBase.gteField,
        validBase.ltField,
        validBase.lteField,
        7,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });
  });

  describe("string validators", () => {
    const valid = {
      lenField: "abcd", // exactly 4
      minLenField: "abc", // >=3
      maxLenField: "abcde", // <=5
      regexField: "hello", // matches /^[a-z]+$/
    };

    it("happy path: valid string inputs -> 200", async () => {
      const res = await saved.stringValidators(
        valid.lenField,
        valid.minLenField,
        valid.maxLenField,
        valid.regexField,
      );
      expect(res.ok, withRes("Expected success", res)).toBe(true);
    });

    it("[len 4] fails when length differs -> 400", async () => {
      const res = await saved.stringValidators(
        "abc",
        valid.minLenField,
        valid.maxLenField,
        valid.regexField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[minlen 3] fails when too short -> 400", async () => {
      const res = await saved.stringValidators(
        valid.lenField,
        "ab",
        valid.maxLenField,
        valid.regexField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[maxlen 5] fails when too long -> 400", async () => {
      const res = await saved.stringValidators(
        valid.lenField,
        valid.minLenField,
        "abcdef",
        valid.regexField,
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });

    it("[regex] fails when value does not match pattern -> 400", async () => {
      const res = await saved.stringValidators(
        valid.lenField,
        valid.minLenField,
        valid.maxLenField,
        "Hello1",
      );
      expect(res.ok).toBe(false);
      expect(res.status).toBe(400);
    });
  });

  describe("uncaught exceptions", () => {
    it("UncaughtException: throwing impl -> 500", async () => {
      const res = await saved.throwingMethod();
      expect(res.ok).toBe(false);
      expect(res.status).toBe(500);
    });
  });
});
