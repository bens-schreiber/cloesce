import { describe, test, expect, afterEach } from "vitest";
import { ModelBuilder, createIdl } from "./builder.js";
import { _cloesceInternal } from "../src/router/router.js";
import { hydrateType } from "../src/router/orm";
import { Cidl } from "../src/cidl.js";

function createHydrateArgs() {
  return {
    idl: { models: {}, poos: {} } as Cidl,
    includeTree: null,
    env: {},
  };
}
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
    const modelMeta = ModelBuilder.model("SparseModel").idPk().col("createdAt", "DateIso").build();

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
      .navP("child", "ChildModel", "One")
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

  test("defaults an absent Many nav to [] on the response path", () => {
    // Arrange
    const childMeta = ModelBuilder.model("ManyChild").idPk().build();
    const parentMeta = ModelBuilder.model("ManyParent")
      .idPk()
      .navP("dogs", "ManyChild", "Many")
      .build();

    const idl = createIdl({ models: [parentMeta, childMeta] });

    // Act: empty include tree, nav absent from body.
    const result = hydrateType(
      { id: 1 },
      { Object: { name: "ManyParent" } },
      { ...createHydrateArgs(), idl, includeTree: {} },
    );

    // Assert
    expect(result.dogs).toEqual([]);
  });

  test("defaults a nested object's absent Many nav to []", () => {
    // Arrange
    const grandchildMeta = ModelBuilder.model("NestedGrandchild").idPk().build();
    const childMeta = ModelBuilder.model("NestedChild")
      .idPk()
      .navP("pups", "NestedGrandchild", "Many")
      .build();
    const parentMeta = ModelBuilder.model("NestedParent")
      .idPk()
      .navP("child", "NestedChild", "One")
      .build();

    const idl = createIdl({ models: [parentMeta, childMeta, grandchildMeta] });

    // Act: include tree walks into child; child's Many nav absent.
    const result = hydrateType(
      { id: 1, child: { id: 2 } },
      { Object: { name: "NestedParent" } },
      {
        ...createHydrateArgs(),
        idl,
        includeTree: { child: {} },
      },
    );

    // Assert
    expect(result.child.pups).toEqual([]);
  });

  test("leaves an absent One nav absent on the response path", () => {
    // Arrange
    const childMeta = ModelBuilder.model("OneChild").idPk().build();
    const parentMeta = ModelBuilder.model("OneParent")
      .idPk()
      .navP("owner", "OneChild", "One")
      .build();

    const idl = createIdl({ models: [parentMeta, childMeta] });

    // Act
    const result = hydrateType(
      { id: 1 },
      { Object: { name: "OneParent" } },
      { ...createHydrateArgs(), idl, includeTree: {} },
    );

    // Assert
    expect("owner" in result).toBe(false);
  });

  test("does not hydrate navigation properties when exclude from include tree", () => {
    // Arrange
    const iso = "2024-03-10T08:00:00.000Z";

    const childMeta = ModelBuilder.model("ChildModel2").idPk().col("createdAt", "DateIso").build();

    const parentMeta = ModelBuilder.model("ParentModel2")
      .idPk()
      .navP("child", "ChildModel2", "One")
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
