// @ts-nocheck
// MissingNavigationPropertyReference

@Model
export class Foo {
  id: number;

  @OneToOne() // missing generic
  bar: Bar;
}
