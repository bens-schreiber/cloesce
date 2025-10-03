export { cloesce } from "./cloesce.js";
export { HttpResult } from "./common.js";
export { modelsFromSql } from "./cloesce.js";

// Compiler hints
export const D1: ClassDecorator = () => {};
export const WranglerEnv: ClassDecorator = () => {};
export const PrimaryKey: PropertyDecorator = () => {};
export const GET: MethodDecorator = () => {};
export const POST: MethodDecorator = () => {};
export const PUT: MethodDecorator = () => {};
export const PATCH: MethodDecorator = () => {};
export const DELETE: MethodDecorator = () => {};
export const DataSource: PropertyDecorator = () => {};
export const OneToMany =
  (_: string): PropertyDecorator =>
  () => {};
export const OneToOne =
  (_: string): PropertyDecorator =>
  () => {};
export const ManyToMany =
  (_: string): PropertyDecorator =>
  () => {};
export const ForeignKey =
  <T>(_: T): PropertyDecorator =>
  () => {};
export const Inject: ParameterDecorator = () => {};

// Include Tree
type Primitive = string | number | boolean | bigint | symbol | null | undefined;
export type IncludeTree<T> = T extends Primitive
  ? never
  : {
      [K in keyof T]?: T[K] extends (infer U)[]
        ? IncludeTree<NonNullable<U>>
        : IncludeTree<NonNullable<T[K]>>;
    };

// Helpers
export function instantiateModelArray<T extends object>(
  data: any,
  ctor: { new (): T },
): T[] {
  if (Array.isArray(data)) {
    return data.map((x) => instantiateModelArray(x, ctor)).flat();
  }
  return [Object.assign(new ctor(), data)];
}
