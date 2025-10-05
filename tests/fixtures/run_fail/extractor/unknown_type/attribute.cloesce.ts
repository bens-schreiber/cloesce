// @ts-nocheck

@D1
class Foo {
  @PrimaryKey
  id: number;
  bad: Bar<number>; // invalid type
}
