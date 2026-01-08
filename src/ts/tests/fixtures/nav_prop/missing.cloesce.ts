// @ts-nocheck
// MissingNavigationPropertyReference

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @OneToOne() // missing generic
  bar: Bar;
}
