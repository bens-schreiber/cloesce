// @ts-nocheck
// UnknownType

@Model()
export class Foo {
  id: number;

  @Post()
  method(valid: number, bad: Bar<number>) {} // invalid param
}
