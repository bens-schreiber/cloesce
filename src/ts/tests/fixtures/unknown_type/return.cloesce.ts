// @ts-nocheck
// UnknownType

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  method(valid: number): Bar<number> {} // invalid return
}
