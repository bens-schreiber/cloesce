// @ts-nocheck
// InvalidApiMethodModifier

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  private method() {}
}
