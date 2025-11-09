// @ts-nocheck
// MissingNavigationPropertyReference

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @OneToOne() // missing generic
  bar: Bar;
}
