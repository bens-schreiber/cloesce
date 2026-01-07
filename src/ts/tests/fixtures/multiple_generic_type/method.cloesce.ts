// @ts-nocheck
// MultipleGenericType

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  method(valid: number, bad: Bar<number, number>) {} // invalid param
}
