// @ts-nocheck
// MultipleGenericType

@Model
export class Foo {
  id: number;

  @POST
  method(valid: number, bad: Bar<number, number>) {} // invalid param
}
