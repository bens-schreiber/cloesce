import type { Handler } from "./types.js";

/** Use as @GET (no parentheses) */
export function GET(_value: Handler, _ctx: ClassMethodDecoratorContext) {}

/** Use as @POST (no parentheses) */
export function POST(_value: Handler, _ctx: ClassMethodDecoratorContext) {}

export function PUT(_value: Handler, _ctx: ClassMethodDecoratorContext) {}

export function PATCH(_value: Handler, _ctx: ClassMethodDecoratorContext) {}

export function DELETE(_value: Handler, _ctx: ClassMethodDecoratorContext) {}

/** Class decorator (no-op) */
export function D1<T extends new (...a: any[]) => object>(
  value: T,
  _ctx: ClassDecoratorContext<T>,
) {
  return value;
}

/** Field decorator (no-op) */
export function PrimaryKey(_v: undefined, _ctx: ClassFieldDecoratorContext) {}
