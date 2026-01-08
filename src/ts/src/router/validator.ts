import {
  CidlType,
  CloesceAst,
  NO_DATA_SOURCE,
  getNavigationPropertyCidlType,
  isNullableType,
} from "../ast";
import { b64ToU8 } from "../ui/common";
import { ConstructorRegistry } from "./router";
import Either from "../either";

/**
 * Runtime type validation, asserting that the structure of a value follows the
 * correlated CidlType.
 *
 * All values must be defined unless `isPartial` is true.
 *
 * Arrays can be left undefined, which will be interpreted as empty.
 *
 * Types will be instantiated in place.
 *
 * If partial, no child types will be instantaited aside from primitives (as of now, just Dates).
 *
 * Blob types will be assumed to be b64 encoded
 *
 * @returns the instantiated value (if applicable). On error, returns null.
 */
export class RuntimeValidator {
  constructor(
    private ast: CloesceAst,
    private ctorReg: ConstructorRegistry,
  ) {}

  static validate(
    value: any,
    cidlType: CidlType,
    ast: CloesceAst,
    ctorReg: ConstructorRegistry,
  ): Either<null, any> {
    return new RuntimeValidator(ast, ctorReg).recurse(value, cidlType, false);
  }

  private recurse(
    value: any,
    cidlType: any,
    isPartial: boolean,
  ): Either<null, any> {
    isPartial ||= typeof cidlType !== "string" && "Partial" in cidlType;

    if (value === undefined) {
      // We will let arrays be undefined and interpret that as an empty array.
      if (typeof cidlType !== "string" && "Array" in cidlType) {
        return Either.right([]);
      }

      return rightIf(() => value, isPartial === true);
    }

    // TODO: consequences of null checking like this? 'null' is passed in
    // as a string for GET requests
    const nullable = isNullableType(cidlType);
    if (value == null || value === "null") {
      // NOTE: Partial types are always nullable.
      return rightIf(() => null, nullable || isPartial);
    }

    // Unwrap nullable types
    if (nullable) {
      cidlType = (cidlType as any).Nullable;
    }

    // JsonValue accepts anything
    if (cidlType === "JsonValue") {
      return Either.right(value);
    }

    // Primitives
    if (typeof cidlType === "string") {
      switch (cidlType) {
        case "Integer":
          return rightIf(() => Number(value), Number.isInteger(Number(value)));
        case "Real":
          return rightIf(() => Number(value), !Number.isNaN(Number(value)));
        case "Text":
          return rightIf(() => String(value), typeof value === "string");
        case "Boolean": {
          if (typeof value === "boolean") return Either.right(value);
          if (value === "true") return Either.right(true);
          if (value === "false") return Either.right(false);
          return Either.left(null);
        }
        case "DateIso": {
          // Instantiate
          const date = new Date(value as string);
          return rightIf(() => date, !isNaN(date.getTime()));
        }
        case "Blob": {
          // Instantiate
          return Either.right(b64ToU8(value));
        }
        default:
          return Either.left(null);
      }
    }

    // Data Sources
    if ("DataSource" in cidlType) {
      const objectName = cidlType.DataSource;
      return rightIf(
        () => value,
        typeof value === "string" &&
          (value === NO_DATA_SOURCE ||
            this.ast.models[objectName]?.data_sources[value] !== undefined),
      );
    }

    const objName = getObjectName(cidlType);

    // Models
    if (objName && this.ast.models[objName]) {
      const model = this.ast.models[objName];
      if (!model || typeof value !== "object") return Either.left(null);
      const valueObj = value as Record<string, unknown>;

      // Validate + instantiate PK
      {
        const pk = model.primary_key;
        const res = this.recurse(valueObj[pk!.name], pk!.cidl_type, isPartial);

        if (res.isLeft()) {
          return res;
        }

        value[pk!.name] = res.unwrap();
      }

      // Validate + instantiate attributes
      for (let i = 0; i < model.columns.length; i++) {
        const attr = model.columns[i];
        const res = this.recurse(
          valueObj[attr.value.name],
          attr.value.cidl_type,
          isPartial,
        );
        if (res.isLeft()) {
          return res;
        }
        value[attr.value.name] = res.unwrap();
      }

      // Validate + instantiate navigation properties
      for (let i = 0; i < model.navigation_properties.length; i++) {
        const nav = model.navigation_properties[i];

        const res = this.recurse(
          valueObj[nav.var_name],
          getNavigationPropertyCidlType(nav),
          isPartial,
        );
        if (res.isLeft()) {
          return res;
        }
        value[nav.var_name] = res.unwrap();
      }

      // Don't instantiate partials
      if (isPartial) {
        return Either.right(value);
      }

      // Instantiate
      return Either.right(Object.assign(new this.ctorReg[objName](), value));
    }

    // Plain old Objects
    if (objName && this.ast.poos[objName]) {
      const poo = this.ast.poos[objName];
      if (!poo || typeof value !== "object") return Either.left(null);
      const valueObj = value as Record<string, unknown>;

      // Validate + instantiate attributes
      for (let i = 0; i < poo.attributes.length; i++) {
        const attr = poo.attributes[i];
        const res = this.recurse(
          valueObj[attr.name],
          attr.cidl_type,
          isPartial,
        );
        if (res.isLeft()) {
          return res;
        }
        value[attr.name] = res.unwrap();
      }

      if (isPartial) {
        return Either.right(value);
      }

      // Instantiate
      return Either.right(Object.assign(new this.ctorReg[objName](), value));
    }

    // Arrays
    if ("Array" in cidlType) {
      if (!Array.isArray(value)) {
        return Either.left(null);
      }

      for (let i = 0; i < value.length; i++) {
        const res = this.recurse(value[i], cidlType.Array, isPartial);
        if (res.isLeft()) {
          return res;
        }

        value[i] = res.unwrap();
      }

      return Either.right(value);
    }

    return Either.left(null);
  }
}

function getObjectName(ty: CidlType) {
  if (typeof ty === "string") {
    return undefined;
  }

  if ("Partial" in ty) {
    return ty.Partial;
  }

  if ("Object" in ty) {
    return ty.Object;
  }

  return undefined;
}

function rightIf(value: () => any, cond: boolean) {
  return cond ? Either.right(value()) : Either.left(null);
}
