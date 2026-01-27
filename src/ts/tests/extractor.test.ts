import { describe, test, expect } from "vitest";
import { Project } from "ts-morph";
import { CidlExtractor } from "../src/extractor/extract";
import { CidlType, DataSource, Service } from "../src/ast";
import { ModelBuilder } from "./builder";

export function cloesceProject(): Project {
  const project = new Project({
    compilerOptions: {
      strict: true,
    },
  });

  project.addSourceFileAtPath("./src/ui/backend.ts");
  return project;
}

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
      import { DataSourceOf, DeepPartial, HttpResult } from "./src/ui/backend";

      class Bar {
        a: number;
      }

      class Foo {
        ds: DataSourceOf<Bar>;
        partial: DeepPartial<Bar>;
        promise: Promise<Bar>;
        arr: Bar[];
        res: HttpResult<Bar>;
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
  test("Finds D1 Database", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
       import { D1Database, KVNamespace } from "@cloudflare/workers-types";
        @WranglerEnv
        class Env {
          db: D1Database;
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
      d1_binding: "db",
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
  test("Finds Include Tree", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      import { IncludeTree } from "./src/ui/backend";
      @Model()
      export class Foo {
      @PrimaryKey
      id: number;
      
      static readonly default: IncludeTree<Foo> = {};
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

    expect(fooModel.data_sources["default"]).toStrictEqual({
      name: "default",
      tree: {},
    } as DataSource);
  });

  test("Infers primary key", () => {
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
    expect(fooModel.primary_key).toEqual({ cidl_type: "Real", name: "id" });

    expect(cidl.models["Bar"]).toBeDefined();
    const barModel = cidl.models["Bar"];
    expect(barModel.primary_key).toEqual({ cidl_type: "Real", name: "bAr_ID" });
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
          foreign_key_reference: "Foo",
        },
      ]),
    );
    expect(barModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: {
            OneToOne: {
              column_reference: "fooId",
            },
          },
          model_reference: "Foo",
          var_name: "foo",
        },
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
              column_reference: "fooId",
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
          foreign_key_reference: "Foo",
        },
      ]),
    );
    expect(barModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: {
            OneToOne: {
              column_reference: "fooId",
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

  test("Explicit 1:1 and 1:M", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      import { OneToOne, OneToMany } from "./src/ui/backend";
      @Model()
      export class Bar {
        id: number;
        
        @ForeignKey(Foo)
        fooId: number;

        @OneToOne<Bar>(b => b.id)
        foo: Foo | undefined;
      }

      @Model()
      export class Foo {
        id: number;

        @OneToMany<Bar>(f => f.fooId)
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
              column_reference: "fooId",
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
          foreign_key_reference: "Foo",
        },
      ]),
    );
    expect(barModel.navigation_properties).toEqual(
      expect.arrayContaining([
        {
          kind: {
            OneToOne: {
              column_reference: "id",
            },
          },
          model_reference: "Foo",
          var_name: "foo",
        },
      ]),
    );
  });

  test("Extracts KV, R2", () => {
    // Arrange
    const project = cloesceProject();
    project.createSourceFile(
      "test.ts",
      `
      import { KValue, Integer, R2ObjectBody } from "./src/ui/backend";
      @Model(["GET", "SAVE"])
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
        allValues: KValue<unknown>[];

        @R2("files/Foo/{id}", "bucket")
        fileData: R2ObjectBody | undefined;

        @R2("files/Foo", "bucket")
        allFiles: R2ObjectBody[] | undefined;
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

            @POST
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

            @POST
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

          const InjectedThingSymbol = Symbol("InjectedThing");
          type InjectedThing = typeof InjectedThingSymbol;

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
