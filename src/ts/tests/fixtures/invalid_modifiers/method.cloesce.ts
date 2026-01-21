// @ts-nocheck
// InvalidApiMethodModifier

@Model()
export class Foo {
  id: number;

  @POST
  private method() {}
}
