import { describe, test, expect, afterEach } from "vitest";
import { Miniflare } from "miniflare";
import { ModelBuilder, createIdl } from "./builder.js";
import { _cloesceInternal } from "../src/router/router.js";
import { hydrateType } from "../src/router/orm";
import { Cidl } from "../src/cidl.js";
import { CloesceResult } from "../src/common.js";

function createHydrateArgs() {
  return {
    idl: { models: {}, poos: {} } as Cidl,
    includeTree: null,
    env: {},
    durable: null,
    promises: [],
  };
}

function mockDurableContext() {
  const store = new Map<string, any>();
  const executed: { query: string; bindings: any[] }[] = [];
  return {
    store,
    executed,
    ctx: {
      state: {
        storage: {
          sql: {
            exec: (query: string, ...bindings: any[]) => {
              executed.push({ query, bindings });
              return { toArray: () => [] };
            },
          },
          kv: {
            get: (key: string) => store.get(key),
            put: (key: string, value: any) => store.set(key, value),
            list: (options?: { prefix?: string }) =>
              [...store.entries()]
                .filter(([key]) => key.startsWith(options?.prefix ?? ""))
                .sort(([a], [b]) => a.localeCompare(b)),
          },
        },
      },
    },
  };
}

describe("hydrateType Tests", () => {
  afterEach(() => {
    _cloesceInternal.RuntimeContainer.dispose();
  });

  describe("Primitive type hydration", () => {
    test("returns null as-is", () => {
      const result = hydrateType(null, "String", createHydrateArgs());
      expect(result).toBeNull();
    });

    test("returns undefined as-is", () => {
      const result = hydrateType(undefined, "String", createHydrateArgs());
      expect(result).toBeUndefined();
    });

    test("hydrates DateIso strings into Date instances", () => {
      const iso = "2024-01-15T12:00:00.000Z";
      const result = hydrateType(iso, "DateIso", createHydrateArgs());
      expect(result).toBeInstanceOf(Date);
      expect(result.toISOString()).toBe(iso);
    });

    test("hydrates Blob number arrays into Uint8Array", () => {
      const arr = [72, 101, 108, 108, 111];
      const result = hydrateType(arr, "Blob", createHydrateArgs());
      expect(result).toBeInstanceOf(Uint8Array);
      expect(Array.from(result)).toEqual(arr);
    });

    test("hydrates Boolean truthy values", () => {
      expect(hydrateType(1, "Boolean", createHydrateArgs())).toBe(true);
      expect(hydrateType(0, "Boolean", createHydrateArgs())).toBe(false);
      expect(hydrateType("true", "Boolean", createHydrateArgs())).toBe(true);
      expect(hydrateType("", "Boolean", createHydrateArgs())).toBe(false);
    });

    test("passes through unknown primitive types unchanged", () => {
      expect(hydrateType("hello", "String", createHydrateArgs())).toBe("hello");
      expect(hydrateType(42, "Int", createHydrateArgs())).toBe(42);
    });
  });

  describe("Array type hydration", () => {
    test("hydrates each element of an array", () => {
      const isos = ["2024-01-01T00:00:00.000Z", "2024-06-15T12:00:00.000Z"];
      const result = hydrateType(isos, { Array: "DateIso" }, createHydrateArgs());
      expect(result).toBeUndefined();
      expect(isos[0]).toBeInstanceOf(Date);
      expect(isos[1]).toBeInstanceOf(Date);
    });

    test("returns empty array when value is not an array", () => {
      const result = hydrateType("not-an-array", { Array: "String" }, createHydrateArgs());
      expect(result).toEqual([]);
    });
  });

  describe("Model column hydration", () => {
    test("hydrates typed columns within a model", async () => {
      // Arrange
      const iso = "2024-03-10T08:00:00.000Z";
      const modelMeta = ModelBuilder.model("TypedColModel")
        .idPk()
        .col("createdAt", "DateIso")
        .col("data", "Blob")
        .build();

      const idl = createIdl({ models: [modelMeta] });

      // Act
      const result = hydrateType(
        { id: 1, createdAt: iso, data: [1, 2, 3] },
        { Object: { name: "TypedColModel" } },
        {
          ...createHydrateArgs(),
          idl,
        },
      );

      // Assert
      expect(result.createdAt).toBeInstanceOf(Date);
      expect(result.createdAt.toISOString()).toBe(iso);
      expect(result.data).toBeInstanceOf(Uint8Array);
      expect(Array.from(result.data)).toEqual([1, 2, 3]);
    });

    test("skips column hydration when column value is undefined", async () => {
      // Arrange
      const modelMeta = ModelBuilder.model("SparseModel")
        .idPk()
        .col("createdAt", "DateIso")
        .build();

      const idl = createIdl({ models: [modelMeta] });

      // Act
      const result = hydrateType(
        { id: 1, createdAt: undefined },
        { Object: { name: "SparseModel" } },
        {
          ...createHydrateArgs(),
          idl,
        },
      );

      // Assert
      expect(result.createdAt).toBeUndefined();
    });
  });

  describe("Navigation property hydration", () => {
    test("hydrates navigation properties and their typed columns when included", () => {
      // Arrange
      const iso = "2024-03-10T08:00:00.000Z";

      const childMeta = ModelBuilder.model("ChildModel").idPk().col("createdAt", "DateIso").build();

      const parentMeta = ModelBuilder.model("ParentModel")
        .idPk()
        .navP("child", "ChildModel", {
          OneToOne: { fields: ["id"] },
        })
        .build();

      const idl = createIdl({ models: [parentMeta, childMeta] });

      const base = {
        id: 1,
        child: {
          id: 2,
          createdAt: iso,
        },
      };

      // Act
      const result = hydrateType(
        base,
        { Object: { name: "ParentModel" } },
        {
          ...createHydrateArgs(),
          idl,
          includeTree: null,
        },
      );

      // Assert
      expect(result.child).toBeDefined();
      expect(result.child.createdAt).toBeInstanceOf(Date);
      expect(result.child.createdAt.toISOString()).toBe(iso);
    });

    test("assembles a route model nav target from this model's route fields", () => {
      // Arrange: a route model whose nav target is built entirely from route values.
      const carMeta = ModelBuilder.model("RouteCar")
        .routeField("ownerId", "Int")
        .routeField("tenant", "String")
        .build();

      const ownerMeta = ModelBuilder.model("RouteOwner")
        .routeField("id", "Int")
        .routeField("org", "String")
        .navP("car", "RouteCar", {
          OneToOne: { fields: ["id", "org"] },
        })
        .build();

      const idl = createIdl({ models: [ownerMeta, carMeta] });

      const base = { id: 1, org: "acme" };

      // Act
      const result = hydrateType(
        base,
        { Object: { name: "RouteOwner" } },
        {
          ...createHydrateArgs(),
          idl,
          includeTree: null,
        },
      );

      // Assert: car is assembled with ownerId <- id, tenant <- org.
      expect(result.car).toEqual({ ownerId: 1, tenant: "acme" });
    });

    test("assembles a route model nav target from a D1 model's columns and primary key", () => {
      // Arrange
      const profileMeta = ModelBuilder.model("Profile")
        .routeField("ownerId", "Int")
        .routeField("tenant", "String")
        .build();

      const userMeta = ModelBuilder.model("User")
        .defaultDb()
        .idPk()
        .col("org", "String")
        .navP("profile", "Profile", {
          OneToOne: { fields: ["id", "org"] },
        })
        .build();

      const idl = createIdl({ models: [userMeta, profileMeta] });
      const base = { id: 7, org: "acme" };

      // Act
      const result = hydrateType(
        base,
        { Object: { name: "User" } },
        {
          ...createHydrateArgs(),
          idl,
          includeTree: null,
        },
      );

      // Assert
      expect(result.profile).toEqual({ ownerId: 7, tenant: "acme" });
    });

    test("does not hydrate navigation properties when exclude from include tree", () => {
      // Arrange
      const iso = "2024-03-10T08:00:00.000Z";

      const childMeta = ModelBuilder.model("ChildModel2")
        .idPk()
        .col("createdAt", "DateIso")
        .build();

      const parentMeta = ModelBuilder.model("ParentModel2")
        .idPk()
        .navP("child", "ChildModel2", {
          OneToOne: { fields: ["id"] },
        })
        .build();

      const idl = createIdl({ models: [parentMeta, childMeta] });

      const base = {
        id: 1,
        child: {
          id: 2,
          createdAt: iso,
        },
      };

      // Act
      const result = hydrateType(
        base,
        { Object: { name: "ParentModel2" } },
        {
          ...createHydrateArgs(),
          idl,
          includeTree: {},
        },
      );

      // Assert
      expect(result.child).toBeDefined();
      expect(result.child.createdAt).not.toBeInstanceOf(Date);
      expect(result.child.createdAt).toBe(iso);
    });
  });
});

describe("ORM Hydrate Tests", () => {
  afterEach(() => {
    _cloesceInternal.RuntimeContainer.dispose();
  });

  test("Hydrate handles KV + R2", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("TestModel")
      .idPk()
      .kvField("config/{id}", "namespace1", "config", { KvObject: "Json" })
      .kvField("config/{id}", "namespace1", "configStream", { KvObject: "Stream" })
      .kvField("emptyConfig", "namespace1", "emptyConfig", { KvObject: "Json" })
      .r2Field("images/{id}", "bucket1", "image")
      .r2Field("emptyImage", "bucket1", "emptyImage")
      .build();

    const base = {
      id: 1,
    };

    const mf = new Miniflare({
      modules: true,
      script: `
            export default {
              async fetch(request, env, ctx) {
                return new Response("Hello Miniflare!");
              }
            }
            `,
      kvNamespaces: ["namespace1"],
      r2Buckets: ["bucket1"],
    });

    const namespace1 = await mf.getKVNamespace("namespace1");
    const baseConfigKV = {
      key: `config/${base.id}`,
      value: { setting: `${base.id} value` },
      metadata: { createdAt: Date.now() },
    };
    await namespace1.put(baseConfigKV.key, JSON.stringify(baseConfigKV.value), {
      metadata: JSON.stringify(baseConfigKV.metadata),
    });

    const bucket1 = await mf.getR2Bucket("bucket1");
    const baseImageObject = {
      key: `images/${base.id}`,
      body: `image data for ${base.id}`,
    };
    await bucket1.put(baseImageObject.key, baseImageObject.body);

    const idl = createIdl({
      models: [modelMeta],
    });

    const env = {
      namespace1: namespace1,
      bucket1: bucket1,
    };

    {
      // Act
      const promises: Promise<CloesceResult<void>>[] = [];
      const noIncludeTree = hydrateType(
        { ...base },
        { Object: { name: "TestModel" } },
        {
          idl,
          includeTree: {},
          env,
          durable: null,
          promises,
        },
      );

      await Promise.all(promises);

      // Assert
      expect(noIncludeTree.config).toBeUndefined();
      expect(noIncludeTree.image).toBeUndefined();
    }

    {
      // Act
      const promises: Promise<CloesceResult<void>>[] = [];
      const fullIncludeTree = hydrateType(
        { ...base },
        { Object: { name: "TestModel" } },
        {
          idl,
          includeTree: {
            config: {},
            configStream: {},
            emptyConfig: {},
            image: {},
            emptyImage: {},
          },
          env,
          durable: null,
          promises,
        },
      );

      await Promise.all(promises);

      // Assert
      expect(fullIncludeTree.config).toEqual({
        raw: baseConfigKV.value,
        metadata: JSON.stringify(baseConfigKV.metadata),
      });
      expect(fullIncludeTree.configStream.value).toBeInstanceOf(ReadableStream);
      expect(fullIncludeTree.emptyConfig.value).toBeNull();

      expect(fullIncludeTree.image).toBeDefined();
      expect(await fullIncludeTree.image.text()).toBe(baseImageObject.body);

      expect(fullIncludeTree.emptyImage).toBeNull();
    }
  });

  test("reads a Durable Object-backed model's KV field directly from the DO's own storage", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Leaderboard")
      .durable("LeaderboardDo", ["tenantId"])
      .kvField("score/{tenantId}", "LeaderboardDo", "score", "Int")
      .build();

    const idl = createIdl({ models: [modelMeta] });
    const { ctx, store } = mockDurableContext();
    store.set("score/7", 42);

    // Act
    const promises: Promise<CloesceResult<void>>[] = [];
    const result = hydrateType(
      { tenantId: 7 },
      { Object: { name: "Leaderboard" } },
      {
        ...createHydrateArgs(),
        idl,
        includeTree: { score: {} },
        durable: ctx,
        promises,
      },
    );

    await Promise.all(promises);

    // Assert: DO storage values are stored directly, with no KValue wrapper.
    expect(result.score).toBe(42);
  });
});
