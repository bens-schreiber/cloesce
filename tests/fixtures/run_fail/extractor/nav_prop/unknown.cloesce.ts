// @ts-nocheck

@D1
class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  foo: IncludeTree<Foo> = { bar: {} }; // bar does not exist
}
