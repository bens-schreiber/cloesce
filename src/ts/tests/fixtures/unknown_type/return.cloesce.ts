// @ts-nocheck
// UnknownType

@Model()
export class Foo {
  id: number;

  @Post()
  method(valid: number): Bar<number> {} // invalid return
}
