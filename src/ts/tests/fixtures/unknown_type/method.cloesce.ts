// @ts-nocheck
// UnknownType

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  method(valid: number, bad: Bar<number>) { } // invalid param
}
