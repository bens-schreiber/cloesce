export type { DeepPartial } from "../common.js";
export { HttpResult, Either } from "../common.js";

// Helpers
export function instantiateObjectArray<T extends object>(
  data: any,
  ctor: { new (): T },
): T[] {
  if (Array.isArray(data)) {
    return data.map((x) => instantiateObjectArray(x, ctor)).flat();
  }
  return [Object.assign(new ctor(), data)];
}
