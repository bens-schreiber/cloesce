import { describe, test, expect, afterEach } from "vitest";
import { Miniflare } from "miniflare";
import { ModelBuilder, createAst } from "./builder";
import { KValue, Orm, Paginated } from "../src/ui/backend.js";
import { _cloesceInternal } from "../src/router/router.js";
import { R2ObjectBody } from "@cloudflare/workers-types";
import { hydrateType } from "../src/router/orm";
import { CloesceAst, CrudListParam } from "../src/ast";

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
        id!: number;
        createdAt!: Date;
        data!: Uint8Array;
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
        id!: number;
        createdAt!: Date;
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
        id!: number;
      }

      const parentMeta = ModelBuilder.model("ParentModel")
        .idPk()
        .col("fk", "Integer", {
          column_name: "id",
          model_name: "ChildModel",
        })
        .navP("child", "ChildModel", { OneToOne: { key_columns: ["fk"] } })
        .build();
      class ParentModel {
        id!: number;
        fk!: number;
        child!: ChildModel;
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
        .col("postId", "Integer", {
          column_name: "id",
          model_name: "PostModel",
        })
        .build();
      class TagModel {
        id!: number;
        postId!: number;
      }

      const postMeta = ModelBuilder.model("PostModel")
        .idPk()
        .navP("tags", "TagModel", {
          OneToMany: { key_columns: ["postId"] },
        })
        .build();
      class PostModel {
        id!: number;
        tags!: TagModel[];
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
      .col("fk", "Integer", {
        column_name: "id",
        model_name: "Depth1Model",
      })
      .build();
    class Depth2Model {
      id!: number;
      fk!: number;
    }

    const depth1ModelMeta = ModelBuilder.model("Depth1Model")
      .idPk()
      .col("name", "Text")
      .navP("depth2", "Depth2Model", {
        OneToMany: { key_columns: ["fk"] },
      })
      .build();
    class Depth1Model {
      id!: number;
      name!: string;
      depth2!: Depth2Model[];
    }

    const modelMeta = ModelBuilder.model("TestModel")
      .idPk()
      .col("fk", "Integer", {
        column_name: "id",
        model_name: "Depth1Model",
      })
      .navP("depth1", "Depth1Model", {
        OneToOne: { key_columns: ["fk"] },
      })
      .build();
    class TestModel {
      id!: number;
      fk!: number;
      depth1!: Depth1Model;
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
        include: {
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
        include: {
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
      id!: number;
      configId!: string;
      config!: KValue<unknown>;
      configStream!: KValue<ReadableStream>;
      configList!: Paginated<KValue<unknown>>;
      emptyConfig!: KValue<unknown>;

      imageId!: string;
      image!: R2ObjectBody;
      imageList!: Paginated<R2ObjectBody>;
      emptyImage!: R2ObjectBody;
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
      const fullIncludeTree: TestModel = await instance.hydrate(TestModel, {
        base,
        keyParams: {
          configId: configId,
          imageId: imageId,
        },
        include: {
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
      .kvObject("cursor-test", "namespace1", "configList", true, "JsonValue")
      .build();

    class CursorModel {
      id!: number;
      configList!: Paginated<KValue<unknown>>;
    }

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
    const ctorReg = {
      CursorModel,
    };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);
    const instance = Orm.fromEnv({ namespace1 });

    // Act
    const hydrated = await instance.hydrate(CursorModel, {
      base: { id: 1 },
      include: { configList: {} },
    });

    // Assert first page
    expect(hydrated.configList.results.length).toBe(1000);
    expect(hydrated.configList.complete).toBe(false);
    expect(hydrated.configList.cursor).toBeTypeOf("string");

    const firstPageKeys = new Set(
      hydrated.configList.results.map((item) => item.key),
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

describe("ORM List Method with DataSource and listParams", () => {
  afterEach(() => {
    _cloesceInternal.RuntimeContainer.dispose();
  });

  test("List with default listParams (LastSeen, Limit)", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Product")
      .defaultDb()
      .idPk()
      .col("name", "Text")
      .col("price", "Integer")
      .build();

    class Product {
      id!: number;
      name!: string;
      price!: number;
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { Product };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    // Create table and insert test data
    await d1
      .prepare(
        `CREATE TABLE Product (id INTEGER PRIMARY KEY, name TEXT, price INTEGER)`,
      )
      .run();
    await d1
      .prepare(
        `INSERT INTO Product (id, name, price) VALUES
         (1, 'Product 1', 100),
         (2, 'Product 2', 200),
         (3, 'Product 3', 300)`,
      )
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const products = await instance.list(Product, {
      lastSeen: { id: 0 },
      limit: 10,
    });

    // Assert
    expect(products).toHaveLength(3);
    expect(products[0]).toBeInstanceOf(Product);
    expect(products[0].id).toBe(1);
    expect(products[1].name).toBe("Product 2");
    expect(products[2].price).toBe(300);
  });

  test("List with explicit custom query and pagination params", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("User")
      .defaultDb()
      .idPk()
      .col("username", "Text")
      .build();

    class User {
      id!: number;
      username!: string;

      static readonly custom = {
        includeTree: {},
        list: () =>
          `SELECT "User"."id", "User"."username" FROM "User" ORDER BY "User"."id" LIMIT ? OFFSET ?`,
        listParams: ["Limit", "Offset"] as CrudListParam[],
      };
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { User };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    await d1
      .prepare(`CREATE TABLE User (id INTEGER PRIMARY KEY, username TEXT)`)
      .run();
    await d1
      .prepare(
        `INSERT INTO User (id, username) VALUES
         (1, 'alice'),
         (2, 'bob'),
         (3, 'charlie'),
         (4, 'diana')`,
      )
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const users = await instance.list(User, {
      include: User.custom,
      limit: 2,
      offset: 1,
    });

    // Assert
    expect(users).toHaveLength(2);
    expect(users[0]).toBeInstanceOf(User);
    expect(users.map((u) => u.username)).toEqual(["bob", "charlie"]);
  });

  test("List supports custom DataSource methods with parameter binding", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Record")
      .defaultDb()
      .idPk()
      .col("data", "Text")
      .build();

    class Record {
      id!: number;
      data!: string;

      static readonly custom = {
        includeTree: {},
        // Custom list with exact parameter binding
        list: () =>
          `SELECT "Record"."id", "Record"."data" FROM "Record" WHERE "Record"."id" > ? LIMIT ?`,
        listParams: ["LastSeen", "Limit"] as CrudListParam[],
      };
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { Record };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    await d1
      .prepare(`CREATE TABLE Record (id INTEGER PRIMARY KEY, data TEXT)`)
      .run();
    await d1
      .prepare(
        `INSERT INTO Record (id, data) VALUES
         (1, 'data1'),
         (2, 'data2')`,
      )
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const records = await instance.list(Record, {
      include: Record.custom,
      lastSeen: { id: 0 },
      limit: 5,
    });

    // Assert
    expect(records).toHaveLength(2);
    expect(records[0]).toBeInstanceOf(Record);
    expect(records[0].id).toBe(1);
    expect(records[1].id).toBe(2);
  });

  test("Get with primary key", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Post")
      .defaultDb()
      .idPk()
      .col("content", "Text")
      .build();

    class Post {
      id!: number;
      content!: string;
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { Post };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    // Create table and insert test data
    await d1
      .prepare(`CREATE TABLE Post (id INTEGER PRIMARY KEY, content TEXT)`)
      .run();
    await d1
      .prepare(
        `INSERT INTO Post (id, content) VALUES
         (1, 'Hello World'),
         (2, 'Second Post')`,
      )
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const post = await instance.get(Post, {
      primaryKey: { id: 1 },
    });

    // Assert
    expect(post).toBeInstanceOf(Post);
    expect(post!.id).toBe(1);
    expect(post!.content).toBe("Hello World");
  });

  test("Get returns null when not found", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Comment")
      .defaultDb()
      .idPk()
      .col("text", "Text")
      .build();

    class Comment {
      id!: number;
      text!: string;
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { Comment };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    // Create table but don't insert data for id=999
    await d1
      .prepare(`CREATE TABLE Comment (id INTEGER PRIMARY KEY, text TEXT)`)
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const comment = await instance.get(Comment, {
      primaryKey: { id: 999 },
    });

    // Assert
    expect(comment).toBeNull();
  });

  test("List with composite primary key", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Enrollment")
      .defaultDb()
      .pk("courseId", "Text")
      .pk("studentId", "Integer")
      .col("status", "Text")
      .build();

    class Enrollment {
      courseId!: string;
      studentId!: number;
      status!: string;
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { Enrollment };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    await d1
      .prepare(
        `CREATE TABLE Enrollment (courseId TEXT, studentId INTEGER, status TEXT, PRIMARY KEY (courseId, studentId))`,
      )
      .run();
    await d1
      .prepare(
        `INSERT INTO Enrollment (courseId, studentId, status) VALUES
         ('course-a', 1, 'active'),
         ('course-a', 2, 'active'),
         ('course-b', 1, 'inactive')`,
      )
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const rows = await instance.list(Enrollment, {
      lastSeen: { courseId: "course-a", studentId: 1 },
      limit: 10,
    });

    // Assert
    expect(rows).toHaveLength(2);
    expect(rows[0]).toBeInstanceOf(Enrollment);
    expect(rows[0].courseId).toBe("course-a");
    expect(rows[0].studentId).toBe(2);
    expect(rows[1].courseId).toBe("course-b");
    expect(rows[1].studentId).toBe(1);
  });

  test("Get with composite primary key", async () => {
    // Arrange
    const modelMeta = ModelBuilder.model("Membership")
      .defaultDb()
      .pk("orgId", "Text")
      .pk("userId", "Integer")
      .col("role", "Text")
      .build();

    class Membership {
      orgId!: string;
      userId!: number;
      role!: string;
    }

    const ast = createAst({ models: [modelMeta] });
    const ctorReg = { Membership };

    await _cloesceInternal.RuntimeContainer.init(ast, ctorReg);

    const mf = new Miniflare({
      modules: true,
      script: `
        export default {
          async fetch(request, env, ctx) {
            return new Response("Hello Miniflare!");
          }
        }
        `,
      d1Databases: ["d1"],
    });

    const d1 = await mf.getD1Database("d1");

    await d1
      .prepare(
        `CREATE TABLE Membership (orgId TEXT, userId INTEGER, role TEXT, PRIMARY KEY (orgId, userId))`,
      )
      .run();
    await d1
      .prepare(
        `INSERT INTO Membership (orgId, userId, role) VALUES
         ('acme', 1, 'owner'),
         ('acme', 2, 'member')`,
      )
      .run();

    const env = { d1 };
    const instance = Orm.fromEnv(env);

    // Act
    const membership = await instance.get(Membership, {
      primaryKey: { orgId: "acme", userId: 2 },
    });

    // Assert
    expect(membership).toBeInstanceOf(Membership);
    expect(membership!.orgId).toBe("acme");
    expect(membership!.userId).toBe(2);
    expect(membership!.role).toBe("member");
  });
});
