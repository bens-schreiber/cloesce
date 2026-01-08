// @ts-nocheck
// InvalidPropertyModifier

@Model
export class Foo {
  @PrimaryKey
  private id: number;
}
