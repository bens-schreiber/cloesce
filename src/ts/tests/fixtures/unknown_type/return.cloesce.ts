// @ts-nocheck
// UnknownType

@Model()
export class Foo {
  id: number;

  @POST
  method(valid: number): Bar<number> {} // invalid return
}
