// @ts-nocheck
// UnknownType

@Model()
export class Foo {
  id: number;

  @POST
  method(valid: number, bad: Bar<number>) {} // invalid param
}
