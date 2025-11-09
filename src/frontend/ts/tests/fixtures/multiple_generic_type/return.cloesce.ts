// @ts-nocheck
// MultipleGenericType

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  method(valid: number): Bar<number, number> {} // invalid return type
}
