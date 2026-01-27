// @ts-nocheck
// MultipleGenericType
class Bar<T, U> {
  a: T;
  b: U;
}

@Model()
export class Foo {
  id: number;

  @POST
  method(valid: number, bad: Bar<number, number>) {} // invalid param
}
