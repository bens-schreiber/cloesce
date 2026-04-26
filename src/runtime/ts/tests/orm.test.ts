import { describe, test, expect, afterEach } from "vitest";
import { Miniflare } from "miniflare";
import { ModelBuilder, createAst } from "./builder";
import { _cloesceInternal } from "../src/router/router.js";
import { hydrateType } from "../src/router/orm";
import { Cidl } from "../src/cidl.js";
import { CloesceResult } from "../src/common.js";

function createHydrateArgs() {
  return {
    ast: { models: {}, poos: {} } as Cidl,
    includeTree: null,
    keyFields: {},
    env: {},
    promises: [],
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
      const result = hydrateType(
        isos,
        { Array: "DateIso" },
        createHydrateArgs(),
      );
      expect(result).toBeUndefined();
      expect(isos[0]).toBeInstanceOf(Date);
      expect(isos[1]).toBeInstanceOf(Date);
    });

    test("returns empty array when value is not an array", () => {
      const result = hydrateType(
        "not-an-array",
        { Array: "String" },
        createHydrateArgs(),
      );
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

      const ast = createAst({ models: [modelMeta] });

      // Act
      const result = hydrateType(
        { id: 1, createdAt: iso, data: [1, 2, 3] },
        { Object: { name: "TypedColModel" } },
        {
          ...createHydrateArgs(),
          ast,
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

      const ast = createAst({ models: [modelMeta] });

      // Act
      const result = hydrateType(
        { id: 1, createdAt: undefined },
        { Object: { name: "SparseModel" } },
        {
          ...createHydrateArgs(),
          ast,
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

      const childMeta = ModelBuilder.model("ChildModel")
        .idPk()
        .col("createdAt", "DateIso")
        .build();

      const parentMeta = ModelBuilder.model("ParentModel")
        .idPk()
        .navP("child", "ChildModel", {
          OneToOne: { columns: ["id"] },
        })
        .build();

      const ast = createAst({ models: [parentMeta, childMeta] });

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
          ast,
          includeTree: null,
        },
      );

      // Assert
      expect(result.child).toBeDefined();
      expect(result.child.createdAt).toBeInstanceOf(Date);
      expect(result.child.createdAt.toISOString()).toBe(iso);
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
          OneToOne: { columns: ["id"] },
        })
        .build();

      const ast = createAst({ models: [parentMeta, childMeta] });

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
          ast,
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
      .keyField("configId")
      .kvField("config/{configId}", "namespace1", "config", false, "Json")
      .kvField(
        "config/{configId}",
        "namespace1",
        "configStream",
        false,
        "Stream",
      )
      .kvField("config", "namespace1", "configList", true, "Json")
      .kvField("emptyConfig", "namespace1", "emptyConfig", false, "Json")
      .keyField("imageId")
      .r2Field("images/{imageId}", "bucket1", "image", false)
      .r2Field("images", "bucket1", "imageList", true)
      .r2Field("emptyImage", "bucket1", "emptyImage", false)
      .build();

    const configId = "some-config-id";
    const imageId = "some-image-id";
    const base = {
      id: 1,
      configId,
      imageId,
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
      key: `config/${configId}`,
      value: { setting: `${configId} value` },
      metadata: { createdAt: Date.now() },
    };
    const otherConfigItem = {
      key: `config/0`,
      value: { setting: `config list item 0` },
    };
    await namespace1.put(baseConfigKV.key, JSON.stringify(baseConfigKV.value), {
      metadata: JSON.stringify(baseConfigKV.metadata),
    });
    await namespace1.put(
      otherConfigItem.key,
      JSON.stringify(otherConfigItem.value),
    );

    const bucket1 = await mf.getR2Bucket("bucket1");
    const baseImageObject = {
      key: `images/${imageId}`,
      body: `image data for ${imageId}`,
    };
    const otherImageObject = {
      key: `images/0`,
      body: `image data for image 0`,
    };
    await bucket1.put(baseImageObject.key, baseImageObject.body);
    await bucket1.put(otherImageObject.key, otherImageObject.body);

    const ast = createAst({
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
          ast,
          includeTree: {},
          keyFields: { configId, imageId },
          env,
          promises,
        },
      );

      await Promise.all(promises);

      // Assert
      expect(noIncludeTree.config).toBeUndefined();
      expect(noIncludeTree.configList).toEqual({
        results: [],
        cursor: null,
        complete: true,
      });
      expect(noIncludeTree.image).toBeUndefined();
      expect(noIncludeTree.imageList).toEqual({
        results: [],
        cursor: null,
        complete: true,
      });
    }

    {
      // Act
      const promises: Promise<CloesceResult<void>>[] = [];
      const fullIncludeTree = hydrateType(
        { ...base },
        { Object: { name: "TestModel" } },
        {
          ast,
          includeTree: {
            config: {},
            configStream: {},
            configList: {},
            emptyConfig: {},
            image: {},
            imageList: {},
            emptyImage: {},
          },
          keyFields: { configId, imageId },
          env,
          promises,
        },
      );

      await Promise.all(promises);

      // Assert
      expect(fullIncludeTree.config).toEqual({
        key: baseConfigKV.key,
        raw: baseConfigKV.value,
        metadata: JSON.stringify(baseConfigKV.metadata),
      });
      expect(fullIncludeTree.configList.results.length).toBe(2);
      expect(fullIncludeTree.configList.complete).toBe(true);
      expect(fullIncludeTree.configList.cursor).toBeNull();
      expect(fullIncludeTree.configList.results).toEqual(
        expect.arrayContaining([
          {
            key: baseConfigKV.key,
            raw: baseConfigKV.value,
            metadata: JSON.stringify(baseConfigKV.metadata),
          },
          {
            key: otherConfigItem.key,
            raw: otherConfigItem.value,
            metadata: null,
          },
        ]),
      );
      expect(fullIncludeTree.configStream.value).toBeInstanceOf(ReadableStream);
      expect(fullIncludeTree.emptyConfig.value).toBeNull();

      expect(fullIncludeTree.image).toBeDefined();
      expect(await fullIncludeTree.image.text()).toBe(baseImageObject.body);
      expect(fullIncludeTree.imageList.results.length).toBe(2);
      expect(fullIncludeTree.imageList.complete).toBe(true);
      expect(fullIncludeTree.imageList.cursor).toBeNull();

      const imageBodies: string[] = [];
      for (const imgObj of fullIncludeTree.imageList.results) {
        imageBodies.push(await imgObj.text());
      }
      expect(imageBodies).toEqual(
        expect.arrayContaining([baseImageObject.body, otherImageObject.body]),
      );

      expect(fullIncludeTree.emptyImage).toBeNull();
    }
  });

  test("KV cursor paginates correctly from hydrated Paginated result", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("CursorModel")
      .idPk()
      .kvField("cursor-test", "namespace1", "configList", true, "Json")
      .build();

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
    });

    const namespace1 = await mf.getKVNamespace("namespace1");
    const total = 1005;
    for (let i = 0; i < total; i++) {
      await namespace1.put(
        `cursor-test/${String(i).padStart(4, "0")}`,
        JSON.stringify({ i }),
      );
    }

    const ast = createAst({ models: [modelMeta] });

    // Act
    const promises: Promise<CloesceResult<void>>[] = [];
    const hydrated = hydrateType(
      { id: 1 },
      { Object: { name: "CursorModel" } },
      {
        ast,
        includeTree: { configList: {} },
        keyFields: {},
        env: { namespace1 },
        promises,
      },
    );

    await Promise.all(promises);

    // Assert first page
    expect(hydrated.configList.results.length).toBe(1000);
    expect(hydrated.configList.complete).toBe(false);
    expect(hydrated.configList.cursor).toBeTypeOf("string");

    const firstPageKeys = new Set(
      hydrated.configList.results.map((item: { key: string }) => item.key),
    );

    // Act on next page using hydrated cursor
    const next = await namespace1.list({
      prefix: "cursor-test",
      cursor: hydrated.configList.cursor!,
    });

    // Assert cursor works for pagination
    expect(next.keys.length).toBe(total - 1000);
    expect(next.list_complete).toBe(true);
    for (const key of next.keys) {
      expect(firstPageKeys.has(key.name)).toBe(false);
    }
  }, 30000);
});
