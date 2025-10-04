// @ts-nocheck

@D1
class Foo {
  @PrimaryKey
  id: number;

  @OneToOne() // missing generic
  bar: Bar;
}
