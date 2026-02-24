// @ts-nocheck
// InvalidApiMethodModifier

@Model()
export class Foo {
  id: number;

  @Post()
  private method() {}
}
