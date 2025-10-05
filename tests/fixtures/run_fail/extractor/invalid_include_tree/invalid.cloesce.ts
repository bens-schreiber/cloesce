// @ts-nocheck

@D1
class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  bar: number; // invalid data source type
}
