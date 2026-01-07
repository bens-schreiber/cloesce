// @ts-nocheck
// InvalidApiMethodModifier

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @POST
  private method() {}
}
