// @ts-nocheck
// UnknownType

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  method(valid: number, bad: Bar<number>) {} // invalid param
}
