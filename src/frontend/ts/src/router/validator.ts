import {
  CidlType,
  CloesceAst,
  NO_DATA_SOURCE,
  getNavigationPropertyCidlType,
  isNullableType,
} from "../ast";
import { Either } from "../ui/common";
import { ModelConstructorRegistry } from "./router";

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
 * Returns the instantiated value (if applicable). On error, returns null.
 */
export class RuntimeValidator {
  static fromJson(
    value: any,
    cidlType: CidlType,
    ast: CloesceAst,
    ctorReg: ModelConstructorRegistry,
  ): Either<null, any> {
    return RuntimeValidator.recurse(value, cidlType, false, ast, ctorReg);
  }

  private static recurse(
    value: any,
    cidlType: any,
    isPartial: boolean,
    ast: CloesceAst,
    ctorReg: ModelConstructorRegistry,
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
      return rightIf(() => null, nullable);
    }

    // Unwrap nullable types
    if (nullable) {
      cidlType = (cidlType as any).Nullable;
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
        case "Boolean":
          if (typeof value === "boolean") return Either.right(value);
          if (value === "true") return Either.right(true);
          if (value === "false") return Either.right(false);
          return Either.left(null);
        case "DateIso":
          // Instantiate
          const date = new Date(value as string);
          return rightIf(() => date, !isNaN(date.getTime()));
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
            ast.models[objectName]?.data_sources[value] !== undefined),
      );
    }

    const objName = getObjectName(cidlType);

    // Models
    if (objName && ast.models[objName]) {
      const model = ast.models[objName];
      if (!model || typeof value !== "object") return Either.left(null);
      const valueObj = value as Record<string, unknown>;

      // Validate + instantiate PK
      {
        const pk = model.primary_key;
        const res = this.recurse(
          valueObj[pk.name],
          pk.cidl_type,
          isPartial,
          ast,
          ctorReg,
        );

        if (res.isLeft()) {
          return res;
        }

        value[pk.name] = res.unwrap();
      }

      // Validate + instantiate attributes
      for (let i = 0; i < model.attributes.length; i++) {
        const attr = model.attributes[i];
        const res = this.recurse(
          valueObj[attr.value.name],
          attr.value.cidl_type,
          isPartial,
          ast,
          ctorReg,
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
          ast,
          ctorReg,
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
      return Either.right(Object.assign(new ctorReg[objName](), value));
    }

    // Plain old Objects
    if (objName && ast.poos[objName]) {
      const poo = ast.poos[objName];
      if (!poo || typeof value !== "object") return Either.left(null);
      const valueObj = value as Record<string, unknown>;

      // Validate + instantiate attributes
      for (let i = 0; i < poo.attributes.length; i++) {
        const attr = poo.attributes[i];
        const res = this.recurse(
          valueObj[attr.name],
          attr.cidl_type,
          isPartial,
          ast,
          ctorReg,
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
      return Either.right(Object.assign(new ctorReg[objName](), value));
    }

    // Arrays
    if ("Array" in cidlType) {
      if (!Array.isArray(value)) {
        return Either.left(null);
      }

      for (let i = 0; i < value.length; i++) {
        const res = this.recurse(
          value[i],
          cidlType.Array,
          isPartial,
          ast,
          ctorReg,
        );
        if (res.isLeft()) {
          return res;
        }

        value[i] = res.unwrap();
      }

      return Either.right(value);
    }

    // HTTP Result
    // TODO: Do we even want to support this?
    if ("HttpResult" in cidlType) {
      return this.recurse(value, cidlType.HttpResult, isPartial, ast, ctorReg);
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
