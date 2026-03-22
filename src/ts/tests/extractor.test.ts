import { describe, test, expect } from "vitest";
import { CidlExtractor } from "../src/extractor/extract";
import { CidlType, Model, Service } from "../src/ast";
import { cloesceProject, ModelBuilder } from "./builder";
import {
  InferenceBuilder,
  InferenceBuilderError,
} from "../src/extractor/infer";

describe("CIDL Type", () => {
  test("Primitives", () => {
    // Arrange
    const project = cloesceProject();

    const sourceFile = project.createSourceFile(
      "test.ts",
      `
      import { Integer } from "./src/ui/backend";
      class Foo {
        isReal: number;
        isInteger: Integer;
        isBool: boolean;
        isDateIso: Date;
      }
      `,
    );

    const attributes = sourceFile
      .getClass("Foo")!
      .getProperties()
      .map((p) => p.getType());

    // Act
    const cidlTypes = attributes.map((a) => {
      const res = CidlExtractor.cidlType(a);
      expect(res.isRight()).toBe(true);
      return res.value as CidlType;
    });

    // Assert
    expect(cidlTypes).toStrictEqual([
      "Real",
      "Integer",
      "Boolean",
      "DateIso",
    ] as CidlType[]);
  });

  test("Nullablility", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
      import { Integer } from "./src/ui/backend";

        class Foo {
          isNull: null,
          isReal: number | null;
          isInteger: Integer | null;
          isBool: boolean | null;
          isDateIso: Date | null;
          notNullable: Foo | undefined;
        }
        `,
    );

    const attributes = sourceFile
      .getClass("Foo")!
      .getProperties()
      .map((p) => p.getType());

    // Act
    const cidlTypes = attributes.map((a) => {
      const res = CidlExtractor.cidlType(a);
      expect(res.isRight()).toBe(true);
      return res.value as CidlType;
    });

    // Assert
    expect(cidlTypes).toStrictEqual([
      { Nullable: "Void" },
      { Nullable: "Real" },
      { Nullable: "Integer" },
      { Nullable: "Boolean" },
      { Nullable: "DateIso" },
      { Object: "Foo" },
    ] as CidlType[]);
  });

  test("Generics", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
      import { DataSource, DeepPartial, HttpResult, KValue, Paginated } from "./src/ui/backend";

      class Bar {
        a: number;
      }

      class Foo {
        ds: DataSource<Bar>;
        partial: DeepPartial<Bar>;
        promise: Promise<Bar>;
        arr: Bar[];
        res: HttpResult<Bar>;
        paginatedKv: Paginated<KValue<Bar>>;
      }
        `,
    );

    const attributes = sourceFile
      .getClass("Foo")!
      .getProperties()
      .map((p) => p.getType());

    // Act
    const cidlTypes = attributes.map((a) => {
      const res = CidlExtractor.cidlType(a);
      expect(res.isRight()).toBe(true);
      return res.value as CidlType;
    });

    // Assert
    expect(cidlTypes).toStrictEqual([
      { DataSource: "Bar" },
      { Partial: "Bar" },
      { Object: "Bar" },
      { Array: { Object: "Bar" } },
      { HttpResult: { Object: "Bar" } },
      { Paginated: { KvObject: { Object: "Bar" } } },
    ] as CidlType[]);
  });
});

describe("Main", () => {
  test("Finds main export", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
    export default async function main(request: Request, env: any, app: CloesceApp, ctx: ExecutionContext): Promise<Response> { }
  `,
    );

    // Act
    const res = CidlExtractor.extract("app", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.main_source).toEqual(sourceFile.getFilePath().toString());
  });
});

describe("WranglerEnv", () => {
  test("Finds bindings", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
       import { D1Database, KVNamespace } from "@cloudflare/workers-types";
        @WranglerEnv
        class Env {
          db: D1Database;
          db2: D1Database;
          kv1: KVNamespace;
          kv2: KVNamespace;
          var1: string;
          var2: number;
        }
      `,
    );

    // Act
    const classDecl = sourceFile.getClass("Env")!;
    const res = CidlExtractor.env(classDecl, sourceFile);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      name: "Env",
      d1_bindings: ["db", "db2"],
      kv_bindings: ["kv1", "kv2"],
      r2_bindings: [],
      vars: {
        var1: "Text",
        var2: "Real",
      },
      source_path: sourceFile.getFilePath().toString(),
    });
  });
});

describe("Model", () => {
  test("Finds Data Sources", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      import { DataSource, GET } from "./src/ui/backend";

      const ds: DataSource<Foo> = {};

      @Model("my_d1")
      export class Foo {
        id: number;

        static readonly ds: DataSource<Foo> = {};

        static readonly dsWithTree: DataSource<Foo> = { 
          includeTree: { bar: {} },
          listParams: ["LastSeen", "Offset", "Limit"],
        };

        @Get(this.ds)
        async thisStaticDs() {}

        @Get(Foo.ds)
        async fooStaticDs() {}

        @Get({})
        async inlineEmptyDs() {}

        @Get({ includeTree: { bar: {} } })
        async inlineDsWithTree() {}

        @Get(ds)
        async externalDs() {}

        @Get()
        async noDs() {}
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("Foo", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Foo"]).toBeDefined();

    const fooModel = cidl.models["Foo"];
    expect(fooModel.d1_binding).toEqual("my_d1");

    expect(fooModel.data_sources).toStrictEqual({
      ds: {
        name: "ds",
        tree: {},
        is_private: false,
        list_params: [],
      },

      dsWithTree: {
        name: "dsWithTree",
        tree: { bar: {} },
        is_private: false,
        list_params: ["LastSeen", "Offset", "Limit"],
      },

      "Foo:inlineEmptyDs": {
        name: "Foo:inlineEmptyDs",
        tree: {},
        is_private: true,
        list_params: [],
      },

      "Foo:inlineDsWithTree": {
        name: "Foo:inlineDsWithTree",
        tree: { bar: {} },
        is_private: true,
        list_params: [],
      },

      "Foo:externalDs": {
        name: "Foo:externalDs",
        tree: {},
        is_private: true,
        list_params: [],
      },
    });

    expect(fooModel.methods["thisStaticDs"]).toBeDefined();
    expect(fooModel.methods["thisStaticDs"].data_source).toEqual("ds");

    expect(fooModel.methods["fooStaticDs"]).toBeDefined();
    expect(fooModel.methods["fooStaticDs"].data_source).toEqual("ds");

    expect(fooModel.methods["inlineEmptyDs"]).toBeDefined();
    expect(fooModel.methods["inlineEmptyDs"].data_source).toEqual(
      "Foo:inlineEmptyDs",
    );

    expect(fooModel.methods["inlineDsWithTree"]).toBeDefined();
    expect(fooModel.methods["inlineDsWithTree"].data_source).toEqual(
      "Foo:inlineDsWithTree",
    );

    expect(fooModel.methods["externalDs"]).toBeDefined();
    expect(fooModel.methods["externalDs"].data_source).toEqual(
      "Foo:externalDs",
    );

    expect(fooModel.methods["noDs"]).toBeDefined();
    expect(fooModel.methods["noDs"].data_source).toBeNull();
  });

  test("Extracts decorated primary keys", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Foo {
        @PrimaryKey
        id: number;
      }

      @Model()
      export class Bar {
        @PrimaryKey
        bAr_ID: number;
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("FooBar", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Foo"]).toBeDefined();

    const fooModel = cidl.models["Foo"];
    expect(fooModel.primary_key_columns).toEqual([
      {
        value: { cidl_type: "Real", name: "id" },
        foreign_key_reference: null,
        unique_ids: [],
        composite_id: null,
      },
    ]);

    expect(cidl.models["Bar"]).toBeDefined();
    const barModel = cidl.models["Bar"];
    expect(barModel.primary_key_columns).toEqual([
      {
        value: { cidl_type: "Real", name: "bAr_ID" },
        foreign_key_reference: null,
        unique_ids: [],
        composite_id: null,
      },
    ]);
  });

  test("Supports multiple primary keys and stacked ForeignKey+PrimaryKey", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Tenant {
        @PrimaryKey
        tenantId: number;

        @PrimaryKey
        regionId: number;
      }

      @Model()
      export class Membership {
        @PrimaryKey
        @ForeignKey<Tenant>(t => t.tenantId)
        tenantId: number;

        @ForeignKey<Tenant>(t => t.regionId)
        @PrimaryKey
        regionId: number;

        @PrimaryKey
        membershipId: number;
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("TenantMembership", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    const membership = cidl.models["Membership"];

    expect(membership.columns).toHaveLength(0);
    expect(membership.primary_key_columns).toHaveLength(3);

    const tenantIdCol = membership.primary_key_columns.find(
      (c) => c.value.name === "tenantId",
    );
    const regionIdCol = membership.primary_key_columns.find(
      (c) => c.value.name === "regionId",
    );
    const membershipIdCol = membership.primary_key_columns.find(
      (c) => c.value.name === "membershipId",
    );

    expect(tenantIdCol).toBeDefined();
    expect(regionIdCol).toBeDefined();
    expect(membershipIdCol).toBeDefined();

    expect(tenantIdCol!.foreign_key_reference).toEqual({
      model_name: "Tenant",
      column_name: "tenantId",
    });
    expect(regionIdCol!.foreign_key_reference).toEqual({
      model_name: "Tenant",
      column_name: "regionId",
    });
    expect(membershipIdCol!.foreign_key_reference).toBeNull();
  });

  test("Infers 1:1 foreign key", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Foo {
        id: number;
      }

      @Model()
      export class Bar {
        id: number;
        
        fooId: number;
        foo: Foo | undefined;
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("FooBar", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Bar"]).toBeDefined();

    const barModel = cidl.models["Bar"];
    expect(barModel.columns).toEqual(
      expect.arrayContaining([
        {
          value: {
            name: "fooId",
            cidl_type: "Real",
          },
          foreign_key_reference: {
            model_name: "Foo",
            column_name: "id",
          },
          unique_ids: [],
          composite_id: null,
        },
      ]),
    );
    expect(barModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: {
            OneToOne: {
              key_columns: ["fooId"],
            },
          },
          model_reference: "Foo",
          var_name: "foo",
        },
      ]),
    );
  });

  test("Infers 1:1 foreign key using referenced model primary key name", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Foo {
        foo_ID: number;
      }

      @Model()
      export class Bar {
        id: number;

        fooFooId: number;
        foo: Foo | undefined;
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("FooBar", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    const barModel = cidl.models["Bar"];
    const fooIdColumn = barModel.columns.find(
      (c) => c.value.name === "fooFooId",
    );

    expect(fooIdColumn).toBeDefined();
    expect(fooIdColumn!.foreign_key_reference).toEqual({
      model_name: "Foo",
      column_name: "foo_ID",
    });
  });

  test("Extracts selector-based @ForeignKey reference", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Foo {
        id: number;
      }

      @Model()
      export class Bar {
        id: number;

        @ForeignKey<Foo>(f => f.id)
        fooId: number;
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("FooBar", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    const barModel = cidl.models["Bar"];

    expect(barModel.columns).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          value: expect.objectContaining({
            name: "fooId",
            cidl_type: "Real",
          }),
          foreign_key_reference: {
            model_name: "Foo",
            column_name: "id",
          },
        }),
      ]),
    );
  });

  test("Infers 1:M foreign key", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Bar {
        id: number;
        
        fooId: number;
        foo: Foo | undefined;
      }

      @Model()
      export class Foo {
        id: number;
        bars: Bar[];
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("FooBar", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Foo"]).toBeDefined();

    const fooModel = cidl.models["Foo"];
    expect(fooModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: {
            OneToMany: {
              key_columns: ["fooId"],
            },
          },
          model_reference: "Bar",
          var_name: "bars",
        },
      ]),
    );

    const barModel = cidl.models["Bar"];
    expect(barModel.columns).toEqual(
      expect.arrayContaining([
        {
          value: {
            name: "fooId",
            cidl_type: "Real",
          },
          foreign_key_reference: {
            model_name: "Foo",
            column_name: "id",
          },
          unique_ids: [],
          composite_id: null,
        },
      ]),
    );
    expect(barModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: {
            OneToOne: {
              key_columns: ["fooId"],
            },
          },
          model_reference: "Foo",
          var_name: "foo",
        },
      ]),
    );
  });

  test("Infers M:M foreign key", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      @Model()
      export class Bar {
        id: number;
        foos: Foo[];
      }

      @Model()
      export class Foo {
        id: number;
        bars: Bar[];
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("FooBar", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Foo"]).toBeDefined();

    const fooModel = cidl.models["Foo"];
    expect(fooModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: "ManyToMany",
          model_reference: "Bar",
          var_name: "bars",
        },
      ]),
    );

    const barModel = cidl.models["Bar"];
    expect(barModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: "ManyToMany",
          model_reference: "Foo",
          var_name: "foos",
        },
      ]),
    );
  });

  test("Extracts KV, R2 attributes", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      import { KValue, Integer, Paginated, R2ObjectBody } from "./src/ui/backend";
      @Crud("GET", "SAVE")
      @Model("db")
      export class Foo {
        @PrimaryKey
        id: Integer;

        name: string;
        real: number;
        boolOrNull: boolean | null;

        @KeyParam
        kvId: string;

        @KV("value/Foo/{id}/{kvId}", "namespace")
        value: KValue<unknown> | undefined;

        @KV("value/Foo", "namespace")
        allValues: Paginated<KValue<unknown>>;

        @R2("files/Foo/{id}", "bucket")
        fileData: R2ObjectBody | undefined;

        @R2("files/Foo", "bucket")
        allFiles: Paginated<R2ObjectBody> | undefined;
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("Foo", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Foo"]).toBeDefined();

    const fooModel = cidl.models["Foo"];
    fooModel.source_path = "";
    expect(fooModel).toEqual(
      ModelBuilder.model("Foo")
        .d1("db")
        .idPk()
        .crud("GET")
        .crud("SAVE")
        .col("name", "Text")
        .col("real", "Real")
        .col("boolOrNull", { Nullable: "Boolean" })
        .keyParam("kvId")
        .kvObject(
          "value/Foo/{id}/{kvId}",
          "namespace",
          "value",
          false,
          "JsonValue",
        )
        .kvObject("value/Foo", "namespace", "allValues", true, "JsonValue")
        .r2Object("files/Foo/{id}", "bucket", "fileData", false)
        .r2Object("files/Foo", "bucket", "allFiles", true)
        .build(),
    );
  });

  test("Extracts KV and R2 in method parameters", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      import { KValue, Integer, R2ObjectBody } from "./src/ui/backend";

      @Model()
      export class Foo {
        id: number;

        @Post()
        async method(
          value: KValue<unknown> | null,
          fileMeta: R2ObjectBody,
        ) {}
      }
      `,
    );

    // Act
    const res = CidlExtractor.extract("Foo", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.models["Foo"]).toBeDefined();

    const fooModel = cidl.models["Foo"];
    expect(fooModel.methods["method"]).toBeDefined();
    const method = fooModel.methods["method"];
    expect(method.parameters).toEqual([
      {
        name: "value",
        cidl_type: { Nullable: { KvObject: "JsonValue" } },
      },
      {
        name: "fileMeta",
        cidl_type: "R2Object",
      },
    ]);
  });
});

describe("Plain Old Objects", () => {
  test("Extracts Plain Old Objects from model references", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
          import { DeepPartial } from "./src/ui/backend";
          export class Bar {
            name: string;
          }

          export class Foo {
            name: string;
          }
          
          export class Baz  {
            name: string;
          }

          @Model
          export class MyModel {
            id: number;

            @Post()
            async method(foo: Foo, bar: Bar | null, baz: DeepPartial<Baz>): Promise<void> { }
          }
          `,
    );

    // Act
    const res = CidlExtractor.extract("project", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.poos).toStrictEqual({
      Foo: {
        name: "Foo",
        attributes: [
          {
            name: "name",
            cidl_type: "Text",
          },
        ],
        source_path: sourceFile.getFilePath().toString(),
      },
      Bar: {
        name: "Bar",
        attributes: [
          {
            name: "name",
            cidl_type: "Text",
          },
        ],
        source_path: sourceFile.getFilePath().toString(),
      },
      Baz: {
        name: "Baz",
        attributes: [
          {
            name: "name",
            cidl_type: "Text",
          },
        ],
        source_path: sourceFile.getFilePath().toString(),
      },
    });
  });

  test("Extracts Plain Old Object from KValue", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
          import { KValue } from "./src/ui/backend";
          export class Bar {
            name: string;
          }

          @Model
          export class MyModel {
            id: number;

            @KV("bar/{id}", "namespace")
            bar: KValue<Bar> | undefined;
          }
          `,
    );

    // Act
    const res = CidlExtractor.extract("MyModel", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.poos).toStrictEqual({
      Bar: {
        name: "Bar",
        attributes: [
          {
            name: "name",
            cidl_type: "Text",
          },
        ],
        source_path: sourceFile.getFilePath().toString(),
      },
    });
  });

  test("Does not extract Plain Old Object without references", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
          export class Bar {
            id: number;
            name: string;
          }

          export class Foo {
            bar: Bar;
            optionalBar: Bar | null;
          }

          @Model
          export class Baz {
            id: number;

            @Post()
            async method(): Promise<void> { }
          }
          `,
    );

    // Act
    const res = CidlExtractor.extract("Foo", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.poos).toStrictEqual({});
  });
});

describe("Services", () => {
  test("Finds injected attributes", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
          @Service
          export class BarService {}

          @Service
          export class FooService {
            barService: BarService;
          }
          `,
    );

    // Act
    const res = CidlExtractor.extract("FooService", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.services["FooService"]).toBeDefined();
    const fooService = cidl.services["FooService"];
    expect(fooService).toEqual({
      name: "FooService",
      attributes: [
        {
          var_name: "barService",
          inject_reference: "BarService",
        },
      ],
      methods: {},
      source_path: sourceFile.getFilePath().toString(),
      initializer: null,
    } as Service);
  });

  test("Finds initializer", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
          import { HttpResult, Inject } from "./src/ui/backend";

          class InjectedThing {
            value: string;
          }

          @Service
          export class BarService {

            // HttpResult<void> return type
            async init(): Promise<HttpResult<void>> {}
          }

          @Service
          export class FooService {
            barService: BarService;
            fooBar: string;

            // Void return type
            async init(@Inject injectedThing: InjectedThing) {
              this.fooBar = "initialized";
              return;
            }
          }
          `,
    );

    // Act
    const res = CidlExtractor.extract("FooService", project);

    // Assert
    expect(res.isRight()).toBe(true);
    const cidl = res.unwrap();
    expect(cidl.services["BarService"]).toBeDefined();
    const barService = cidl.services["BarService"];
    expect(barService.initializer).toEqual([]);
    expect(cidl.services["FooService"]).toBeDefined();
    const fooService = cidl.services["FooService"];
    expect(fooService.initializer).toEqual(["InjectedThing"]);
  });
});

describe("InferenceBuilder", () => {
  test("Referenced model has no primary keys => MissingPrimaryKeys", () => {
    // Arrange
    const userModel = ModelBuilder.model("User")
      .idPk()
      .col("profileId", "Integer")
      .build();

    const profileModel = ModelBuilder.model("Profile")
      .col("name", "Text")
      .build();

    const models: Record<string, Model> = {
      User: userModel,
      Profile: profileModel,
    };

    const builder = new InferenceBuilder();
    builder.addOneToOne({
      modelName: "User",
      propertyName: "profile",
      referencedModelName: "Profile",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const userErrors = errors["User"];
    expect(userErrors).toHaveLength(1);
    expect(userErrors[0]).toEqual(InferenceBuilderError.MissingPrimaryKeys);
    expect(userModel.navigation_properties).toHaveLength(0);
  });

  test("Source model has no primary keys => MissingPrimaryKeys", () => {
    // Arrange
    const departmentModel = ModelBuilder.model("Department")
      .col("name", "Text")
      .build();

    const employeeModel = ModelBuilder.model("Employee")
      .idPk()
      .col("name", "Text")
      .build();

    const models: Record<string, Model> = {
      Department: departmentModel,
      Employee: employeeModel,
    };

    const builder = new InferenceBuilder();
    builder.addMany({
      modelName: "Department",

      propertyName: "employees",
      referencedModelName: "Employee",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const departmentErrors = errors["Department"];
    expect(departmentErrors).toHaveLength(1);
    expect(departmentErrors[0]).toEqual(
      InferenceBuilderError.MissingPrimaryKeys,
    );
    expect(departmentModel.navigation_properties).toHaveLength(0);
  });

  test("No columns match the referenced primary keys => MissingMatchingColumns", () => {
    // Arrange
    const userModel = ModelBuilder.model("User")
      .idPk()
      .col("name", "Text")
      .col("email", "Text")
      .build();

    const profileModel = ModelBuilder.model("Profile")
      .idPk()
      .col("bio", "Text")
      .build();

    const models: Record<string, Model> = {
      User: userModel,
      Profile: profileModel,
    };

    const builder = new InferenceBuilder();
    builder.addOneToOne({
      modelName: "User",
      propertyName: "profile",
      referencedModelName: "Profile",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const userErrors = errors["User"];
    expect(userErrors).toHaveLength(1);
    expect(userErrors[0]).toEqual(InferenceBuilderError.MissingMatchingColumns);
    expect(userModel.navigation_properties).toHaveLength(0);
  });

  test("Partial match not enough => MissingMatchingColumns", () => {
    // Arrange
    const orderModel = ModelBuilder.model("Order")
      .pk("userId", "Integer")
      .pk("productId", "Integer")
      .col("shippingAddressUserId", "Integer")
      .build();

    const addressModel = ModelBuilder.model("Address")
      .pk("userId", "Integer")
      .pk("productId", "Integer")
      .col("street", "Text")
      .build();

    const models: Record<string, Model> = {
      Order: orderModel,
      Address: addressModel,
    };

    const builder = new InferenceBuilder();
    builder.addOneToOne({
      modelName: "Order",
      propertyName: "shippingAddress",
      referencedModelName: "Address",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const orderErrors = errors["Order"];
    expect(orderErrors).toHaveLength(1);
    expect(orderErrors[0]).toEqual(
      InferenceBuilderError.MissingMatchingColumns,
    );
    expect(orderModel.navigation_properties).toHaveLength(0);
  });

  test("No FK or 1:1 relationships back => MissingMatchingColumns", () => {
    // Arrange
    const departmentModel = ModelBuilder.model("Department")
      .idPk()
      .col("name", "Text")
      .build();

    const employeeModel = ModelBuilder.model("Employee")
      .idPk()
      .col("name", "Text")
      .build();

    const models: Record<string, Model> = {
      Department: departmentModel,
      Employee: employeeModel,
    };

    const builder = new InferenceBuilder();
    builder.addMany({
      modelName: "Department",
      propertyName: "employees",
      referencedModelName: "Employee",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const departmentErrors = errors["Department"];
    expect(departmentErrors).toHaveLength(1);
    expect(departmentErrors[0]).toEqual(
      InferenceBuilderError.MissingMatchingColumns,
    );
    expect(departmentModel.navigation_properties).toHaveLength(0);
  });

  test("Some but not all foreign keys defined => IncompleteForeignKeys", () => {
    // Arrange
    const orderModel = ModelBuilder.model("Order")
      .idPk()
      .col("addressUserId", "Integer", {
        model_name: "Address",
        column_name: "userId",
      })
      .col("addressPostalCode", "Text")
      .build();

    const addressModel = ModelBuilder.model("Address")
      .pk("userId", "Integer")
      .pk("postalCode", "Text")
      .col("street", "Text")
      .build();

    const models: Record<string, Model> = {
      Order: orderModel,
      Address: addressModel,
    };

    const builder = new InferenceBuilder();
    builder.addOneToOne({
      modelName: "Order",
      propertyName: "address",
      referencedModelName: "Address",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const orderErrors = errors["Order"];
    expect(orderErrors).toHaveLength(1);
    expect(orderErrors[0]).toEqual(InferenceBuilderError.IncompleteForeignKeys);
    expect(orderModel.navigation_properties).toHaveLength(0);
  });

  test("Existing foreign keys target wrong model => IncorrectForeignKeyTarget", () => {
    // Arrange
    const orderModel = ModelBuilder.model("Order")
      .idPk()
      .col("addressUserId", "Integer", {
        model_name: "User",
        column_name: "id",
      })
      .col("addressPostalCode", "Text", {
        model_name: "User",
        column_name: "postalCode",
      })
      .build();

    const addressModel = ModelBuilder.model("Address")
      .pk("userId", "Integer")
      .pk("postalCode", "Text")
      .col("street", "Text")
      .build();

    const userModel = ModelBuilder.model("User")
      .idPk()
      .col("postalCode", "Text")
      .build();

    const models: Record<string, Model> = {
      Order: orderModel,
      Address: addressModel,
      User: userModel,
    };

    const builder = new InferenceBuilder();
    builder.addOneToOne({
      modelName: "Order",
      propertyName: "address",
      referencedModelName: "Address",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const orderErrors = errors["Order"];
    expect(orderErrors).toHaveLength(1);
    expect(orderErrors[0]).toEqual(
      InferenceBuilderError.IncorrectForeignKeyTarget,
    );
    expect(orderModel.navigation_properties).toHaveLength(0);
  });

  test("Multiple column sets could form the relationship => AmbiguousRelationship", () => {
    // Arrange
    const userModel = ModelBuilder.model("User")
      .idPk()
      .col("profileId", "Integer")
      .col("profile_id", "Integer")
      .build();

    const profileModel = ModelBuilder.model("Profile")
      .idPk()
      .col("bio", "Text")
      .build();

    const models: Record<string, Model> = {
      User: userModel,
      Profile: profileModel,
    };

    const builder = new InferenceBuilder();
    builder.addOneToOne({
      modelName: "User",
      propertyName: "profile",
      referencedModelName: "Profile",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const userErrors = errors["User"];
    expect(userErrors).toHaveLength(1);
    expect(userErrors[0]).toEqual(InferenceBuilderError.AmbiguousRelationship);
    expect(userModel.navigation_properties).toHaveLength(0);
  });

  test("Multiple back references for ManyToMany => AmbiguousRelationship", () => {
    // Arrange
    const studentModel = ModelBuilder.model("Student").idPk().build();

    const courseModel = ModelBuilder.model("Course").idPk().build();

    const models: Record<string, Model> = {
      Student: studentModel,
      Course: courseModel,
    };

    const builder = new InferenceBuilder();

    builder.addMany({
      modelName: "Student",
      propertyName: "courses",
      referencedModelName: "Course",
    });

    builder.addMany({
      modelName: "Course",
      propertyName: "students",
      referencedModelName: "Student",
    });

    builder.addMany({
      modelName: "Course",
      propertyName: "enrolledStudents",
      referencedModelName: "Student",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const studentErrors = errors["Student"];
    expect(studentErrors).toHaveLength(1);
    expect(studentErrors[0]).toEqual(
      InferenceBuilderError.AmbiguousRelationship,
    );
    expect(studentModel.navigation_properties).toHaveLength(0);

    const courseErrors = errors["Course"];
    expect(courseErrors).toHaveLength(2);
    expect(courseModel.navigation_properties).toHaveLength(0);
  });

  test("Multiple ways to reference (multiple FK sets) => AmbiguousRelationship - ", () => {
    // Arrange
    const departmentModel = ModelBuilder.model("Department")
      .idPk()
      .col("name", "Text")
      .build();

    const employeeModel = ModelBuilder.model("Employee")
      .idPk()
      .col("departmentId", "Integer", {
        model_name: "Department",
        column_name: "id",
      })
      .col("dept_id", "Integer", {
        model_name: "Department",
        column_name: "id",
      })
      .build();

    const models: Record<string, Model> = {
      Department: departmentModel,
      Employee: employeeModel,
    };

    const builder = new InferenceBuilder();
    builder.addMany({
      modelName: "Department",
      propertyName: "employees",
      referencedModelName: "Employee",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const departmentErrors = errors["Department"];
    expect(departmentErrors).toHaveLength(1);
    expect(departmentErrors[0]).toEqual(
      InferenceBuilderError.AmbiguousRelationship,
    );
    expect(departmentModel.navigation_properties).toHaveLength(0);
  });

  test("Multiple 1:1 nav properties back => AmbiguousRelationship", () => {
    // Arrange
    const departmentModel = ModelBuilder.model("Department")
      .idPk()
      .col("name", "Text")
      .build();

    const employeeModel = ModelBuilder.model("Employee")
      .idPk()
      .col("primaryDepartmentId", "Integer", {
        model_name: "Department",
        column_name: "id",
      })
      .col("secondaryDepartmentId", "Integer", {
        model_name: "Department",
        column_name: "id",
      })
      .navP("primaryDepartment", "Department", {
        OneToOne: { key_columns: ["primaryDepartmentId"] },
      })
      .navP("secondaryDepartment", "Department", {
        OneToOne: { key_columns: ["secondaryDepartmentId"] },
      })
      .build();

    const models: Record<string, Model> = {
      Department: departmentModel,
      Employee: employeeModel,
    };

    const builder = new InferenceBuilder();
    builder.addMany({
      modelName: "Department",
      propertyName: "employees",
      referencedModelName: "Employee",
    });

    // Act
    const errors = builder.build(models);

    // Assert
    const departmentErrors = errors["Department"];
    expect(departmentErrors).toHaveLength(1);
    expect(departmentErrors[0]).toEqual(
      InferenceBuilderError.AmbiguousRelationship,
    );
    expect(departmentModel.navigation_properties).toHaveLength(0);
  });
});
