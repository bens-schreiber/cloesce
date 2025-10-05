// @ts-nocheck

@D1
class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  foo: IncludeTree<Foo> = { id: {} }; // id is not a model
}
