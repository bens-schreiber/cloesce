import { describe, test, expect, afterEach } from "vitest";
import { Miniflare } from "miniflare";
import { ModelBuilder, createAst } from "./builder";
import { KValue, Orm } from "../src/ui/backend.js";
import { _cloesceInternal } from "../src/router/router.js";
import { R2ObjectBody } from "@cloudflare/workers-types";
import { hydrateType } from "../src/router/orm";
import { CloesceAst } from "../src/ast";

function createHydrateArgs() {
  return {
    ast: { models: {}, poos: {} } as CloesceAst,
    ctorReg: {},
    includeTree: null,
    keyParams: {},
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
      const result = hydrateType(null, "Text", createHydrateArgs());
      expect(result).toBeNull();
    });

    test("returns undefined as-is", () => {
      const result = hydrateType(undefined, "Text", createHydrateArgs());
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
      expect(hydrateType("hello", "Text", createHydrateArgs())).toBe("hello");
      expect(hydrateType(42, "Integer", createHydrateArgs())).toBe(42);
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
        { Array: "Text" },
        createHydrateArgs(),
      );
      expect(result).toEqual([]);
    });
  });

  describe("Model column hydration", () => {
    test("hydrates typed columns within a model instance", async () => {
      // Arrange
      const modelMeta = ModelBuilder.model("TypedColModel")
        .idPk()
        .col("createdAt", "DateIso")
        .col("data", "Blob")
        .build();

      class TypedColModel {
        id: number;
        createdAt: Date;
        data: Uint8Array;
      }

      const ast = createAst({ models: [modelMeta] });
      const ctorReg = { TypedColModel };

      // Act
      const result = hydrateType(
        { id: 1, createdAt: "2024-03-10T08:00:00.000Z", data: [1, 2, 3] },
        { Object: TypedColModel.name },
        {
          ...createHydrateArgs(),
          ast,
          ctorReg,
        },
      );

      // Assert
      expect(result).toBeInstanceOf(TypedColModel);
      expect(result.createdAt).toBeInstanceOf(Date);
      expect(result.createdAt.toISOString()).toBe("2024-03-10T08:00:00.000Z");
      expect(result.data).toBeInstanceOf(Uint8Array);
      expect(Array.from(result.data)).toEqual([1, 2, 3]);
    });

    test("skips column hydration when column value is undefined", async () => {
      // Arrange
      const modelMeta = ModelBuilder.model("SparseModel")
        .idPk()
        .col("createdAt", "DateIso")
        .build();

      class SparseModel {
        id: number;
        createdAt: Date;
      }

      const ast = createAst({ models: [modelMeta] });
      const ctorReg = { SparseModel };

      // Act
      const result = hydrateType(
        { id: 1, createdAt: undefined },
        { Object: SparseModel.name },
        {
          ...createHydrateArgs(),
          ast,
          ctorReg,
        },
      );

      // Assert
      expect(result).toBeInstanceOf(SparseModel);
      expect(result.createdAt).toBeUndefined();
    });
  });

  describe("Navigation property include tree behavior", () => {
    test("nav property is left unhydrated when omitted from include tree", async () => {
      // Arrange
      const childMeta = ModelBuilder.model("ChildModel").idPk().build();
      class ChildModel {
        id: number;
      }

      const parentMeta = ModelBuilder.model("ParentModel")
        .idPk()
        .col("fk", "Integer", "ChildModel")
        .navP("child", "ChildModel", { OneToOne: { column_reference: "fk" } })
        .build();
      class ParentModel {
        id: number;
        fk: number;
        child: ChildModel;
      }

      const ast = createAst({ models: [parentMeta, childMeta] });
      const ctorReg = { ParentModel, ChildModel };

      // Act
      const result = hydrateType(
        { id: 1, fk: 2, child: { id: 2 } },
        { Object: ParentModel.name },
        {
          ...createHydrateArgs(),
          ast,
          ctorReg,
          includeTree: {}, // empty include tree, so nav props should not be hydrated
        },
      );

      // Assert
      expect(result).toBeInstanceOf(ParentModel);
      expect(result.child).not.toBeInstanceOf(ChildModel);
    });

    test("OneToMany nav property hydrates array of instances", async () => {
      // Arrange
      const tagMeta = ModelBuilder.model("TagModel")
        .idPk()
        .col("postId", "Integer", "PostModel")
        .build();
      class TagModel {
        id: number;
        postId: number;
      }

      const postMeta = ModelBuilder.model("PostModel")
        .idPk()
        .navP("tags", "TagModel", {
          OneToMany: { column_reference: "postId" },
        })
        .build();
      class PostModel {
        id: number;
        tags: TagModel[];
      }

      const ast = createAst({ models: [postMeta, tagMeta] });
      const ctorReg = { PostModel, TagModel };
      const obj = {
        id: 1,
        tags: [
          { id: 10, postId: 1 },
          { id: 11, postId: 1 },
        ],
      };

      // Act
      const result = hydrateType(
        obj,
        { Object: PostModel.name },
        {
          ...createHydrateArgs(),
          ast,
          ctorReg,
          includeTree: { tags: {} }, // include 'tags' nav prop for hydration
        },
      );

      // Assert
      expect(result).toBeInstanceOf(PostModel);
      expect(result.tags).toHaveLength(2);
      expect(result.tags[0]).toBeInstanceOf(TagModel);
      expect(result.tags[1]).toBeInstanceOf(TagModel);
    });
  });
});

describe("ORM Hydrate Tests", () => {
  afterEach(() => {
    _cloesceInternal.RuntimeContainer.dispose();
  });

  test("Hydrate instantiates Models and their navigation properties", async () => {
    // // Arrange
    const depth2ModelMeta = ModelBuilder.model("Depth2Model")
      .idPk()
      .col("fk", "Integer", "Depth1Model")
      .build();
    class Depth2Model {
      id: number;
      fk: number;
    }

    const depth1ModelMeta = ModelBuilder.model("Depth1Model")
      .idPk()
      .col("name", "Text")
      .navP("depth2", "Depth2Model", {
        OneToMany: { column_reference: "fk" },
      })
      .build();
    class Depth1Model {
      id: number;
      name: string;
      depth2: Depth2Model[];
    }

    const modelMeta = ModelBuilder.model("TestModel")
      .idPk()
      .col("fk", "Integer", "Depth1Model")
      .navP("depth1", "Depth1Model", {
        OneToOne: { column_reference: "fk" },
      })
      .build();
    class TestModel {
      id: number;
      fk: number;
      depth1: Depth1Model;
    }

    const base = {
      id: 1,
      fk: 2,
      depth1: {
        id: 2,
        name: "Depth 1",
        depth2: [
          { id: 3, fk: 2 },
          { id: 4, fk: 2 },
        ],
      },
    };

    const ast = createAst({
      models: [modelMeta, depth1ModelMeta, depth2ModelMeta],
    });

    const ctorReg = {
      TestModel: TestModel,
      Depth1Model: Depth1Model,
      Depth2Model: Depth2Model,
    };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const env = {};
    const instance = Orm.fromEnv(env);

    {
      // Act
      const noIncludeTree = await instance.hydrate(TestModel, {
        base,
      });

      // Assert
      expect(noIncludeTree).toBeInstanceOf(TestModel);
      expect(noIncludeTree.depth1).not.toBeInstanceOf(Depth1Model);
    }

    {
      // Act
      const depth1IncludeTree = await instance.hydrate(TestModel, {
        base,
        includeTree: {
          depth1: {},
        },
      });

      // Assert
      expect(depth1IncludeTree).toBeInstanceOf(TestModel);
      expect(depth1IncludeTree.depth1).toBeInstanceOf(Depth1Model);
    }

    {
      // Act
      const fullIncludeTree = await instance.hydrate(TestModel, {
        base,
        includeTree: {
          depth1: {
            depth2: {},
          },
        },
      });

      // Assert
      expect(fullIncludeTree).toBeInstanceOf(TestModel);
      expect(fullIncludeTree.depth1).toBeInstanceOf(Depth1Model);
      expect(fullIncludeTree.depth1.depth2[0]).toBeInstanceOf(Depth2Model);
      expect(fullIncludeTree.depth1.depth2[1]).toBeInstanceOf(Depth2Model);
    }
  });

  test("Hydrate handles KV + R2", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("TestModel")
      .idPk()
      .keyParam("configId")
      .kvObject("config/{configId}", "namespace1", "config", false, "JsonValue")
      .kvObject(
        "config/{configId}",
        "namespace1",
        "configStream",
        false,
        "Stream",
      )
      .kvObject("config", "namespace1", "configList", true, "JsonValue")
      .kvObject("emptyConfig", "namespace1", "emptyConfig", false, "JsonValue")
      .keyParam("imageId")
      .r2Object("images/{imageId}", "bucket1", "image", false)
      .r2Object("images", "bucket1", "imageList", true)
      .r2Object("emptyImage", "bucket1", "emptyImage", false)
      .build();

    class TestModel {
      id: number;
      configId: string;
      config: KValue<unknown>;
      configStream: KValue<ReadableStream>;
      configList: KValue<unknown>[];
      emptyConfig: KValue<unknown>;

      imageId: string;
      image: R2ObjectBody;
      imageList: R2ObjectBody[];
      emptyImage: R2ObjectBody;
    }

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

    const ctorReg = {
      TestModel: TestModel,
    };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const env = {
      namespace1: namespace1,
      bucket1: bucket1,
    };

    const instance = Orm.fromEnv(env);

    {
      // Act
      const noIncludeTree = await instance.hydrate(TestModel, {
        base,
        keyParams: {
          configId: configId,
          imageId: imageId,
        },
      });

      // Assert
      expect(noIncludeTree).toBeInstanceOf(TestModel);
      expect(noIncludeTree.config).toBeUndefined();
      expect(noIncludeTree.configList).toEqual([]);
      expect(noIncludeTree.image).toBeUndefined();
      expect(noIncludeTree.imageList).toEqual([]);
    }

    {
      // Act
      const fullIncludeTree: TestModel = await instance.hydrate(TestModel, {
        base,
        keyParams: {
          configId: configId,
          imageId: imageId,
        },
        includeTree: {
          config: {},
          configStream: {},
          configList: {},
          emptyConfig: {},
          image: {},
          imageList: {},
          emptyImage: {},
        },
      });

      // Assert
      expect(fullIncludeTree).toBeInstanceOf(TestModel);
      expect(fullIncludeTree.config).toEqual({
        key: baseConfigKV.key,
        raw: baseConfigKV.value,
        metadata: JSON.stringify(baseConfigKV.metadata),
      });
      expect(fullIncludeTree.configList.length).toBe(2);
      expect(fullIncludeTree.configList).toEqual(
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
      expect(fullIncludeTree.imageList.length).toBe(2);

      const imageBodies: string[] = [];
      for (const imgObj of fullIncludeTree.imageList) {
        imageBodies.push(await imgObj.text());
      }
      expect(imageBodies).toEqual(
        expect.arrayContaining([baseImageObject.body, otherImageObject.body]),
      );

      expect(fullIncludeTree.emptyImage).toBeNull();
    }
  });
});
