import { describe, test, expect } from "vitest";
import { Project } from "ts-morph";
import {
  CidlExtractor,
  ModelExtractor,
  ServiceExtractor,
} from "../src/extractor/extract";
import { CidlType, DataSource, Service } from "../src/ast";
import { ModelBuilder } from "./builder";


function cloesceProject(): Project {
  const project = new Project({
    compilerOptions: {
      strict: true,
    },
  });

  project.addSourceFileAtPath("./src/ui/common.ts");
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

describe("Middleware", () => {
  test("Finds app export", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
    import { CloesceApp } from "./src/ui/backend";
    const app = new CloesceApp();
    export default app;
  `,
    );

    // Act
    const res = CidlExtractor.app(sourceFile);

    // Assert
    expect(res.isRight()).toBe(true);
  });
});

describe("WranglerEnv", () => {
  test("Finds D1 Database", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
       import { D1Database } from "@cloudflare/workers-types";
        @WranglerEnv
        class Env {
          db: D1Database;
        }
      `,
    );

    // Act
    const classDecl = sourceFile.getClass("Env")!;
    const res = CidlExtractor.env(classDecl, sourceFile);

    // Assert
    expect(res.isRight()).toBe(true);
  });
});

describe("Model", () => {
  test("Finds Include Tree", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
      import { IncludeTree } from "./src/ui/backend";
      @Model
      class Foo {
      @PrimaryKey
      id: number;

      @DataSource
      static readonly default: IncludeTree<Foo> = {};
      }
      `,
    );

    // Act
    const classDecl = sourceFile.getClass("Foo")!;
    const res = ModelExtractor.extract(classDecl, sourceFile);

    // Assert
    expect(res.isRight()).toBe(true);

    expect(res.unwrap().data_sources["default"]).toStrictEqual({
      name: "default",
      tree: {},
    } as DataSource);
  });

  test("Extracts Primary Key, Columns, Key Params and KV", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
      import { KValue, Integer } from "./src/ui/backend";
      @Model
      class Foo {
        @PrimaryKey
        id: Integer;

        name: string;
        real: number;
        boolOrNull: boolean | null;

        @KeyParam
        kvId: string;

        @KV("value/Foo/{id}/{kvId}", "namespace")
        value: KValue<unknown>;
      }
      `,
    );

    // Act
    const classDecl = sourceFile.getClass("Foo")!;
    const res = ModelExtractor.extract(classDecl, sourceFile);

    // Assert
    expect(res.isRight(), `Error: ${JSON.stringify(res)}`).toBe(true);

    res.unwrap().source_path = "";
    expect(res.unwrap()).toEqual(ModelBuilder
      .model("Foo")
      .idPk()
      .col("name", "Text")
      .col("real", "Real")
      .col("boolOrNull", { Nullable: "Boolean" })
      .keyParam("kvId")
      .kvObject("value/Foo/{id}/{kvId}", "namespace", "value", "JsonValue")
      .build()
    )
  })
});

describe("Services", () => {
  test("Finds injected attributes", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
          @Service
          class BarService {}

          @Service
          class FooService {
            barService: BarService;
          }
          `,
    );

    // Act
    const classDecl = sourceFile.getClass("FooService")!;
    const res = ServiceExtractor.extract(classDecl, sourceFile);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      name: "FooService",
      attributes: [
        {
          var_name: "barService",
          inject_reference: "BarService",
        },
      ],
      methods: {},
      source_path: sourceFile.getFilePath().toString(),
    } as Service);
  });
});

