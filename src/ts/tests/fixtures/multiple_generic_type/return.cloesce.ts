// @ts-nocheck
// MultipleGenericType

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  method(valid: number): Bar<number, number> {} // invalid return type
}
