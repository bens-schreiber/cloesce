import { describe, test, expect } from "vitest";
import { Project } from "ts-morph";
import { CidlExtractor } from "../src/extractor/extract";
import { CidlType, DataSource, Model } from "../src/common";

function cloesceProject(): Project {
  const project = new Project({
    compilerOptions: {
      strict: true,
    },
  });

  project.addSourceFileAtPath("./src/common.ts");
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
      import { DataSourceOf, DeepPartial } from "./src/ui/backend";

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
    console.log(res.value);
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
       import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
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

describe("Data Source", () => {
  test("Finds Include Tree", () => {
    // Arrange
    const project = cloesceProject();
    const sourceFile = project.createSourceFile(
      "test.ts",
      `
      import { IncludeTree } from "./src/ui/backend";
      @D1
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
    const res = CidlExtractor.model(classDecl, sourceFile);

    // Assert
    expect(res.isRight()).toBe(true);

    expect(res.unwrap().data_sources["default"]).toStrictEqual({
      name: "default",
      tree: {},
    } as DataSource);
  });
});
