import { describe, test, expect, afterEach } from "vitest";
import { Miniflare } from "miniflare";
import { ModelBuilder, createAst } from "./builder";
import { KValue, Orm } from "../src/ui/backend.js";
import { _cloesceInternal } from "../src/router/router.js";

import { R2ObjectBody } from "@cloudflare/workers-types";

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
      const fullIncludeTree: TestModel = await instance.hydrate(
        TestModel,

        {
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
        },
      );

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
