// @ts-nocheck

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  foo: IncludeTree<Foo> = { bar: {} }; // bar does not exist
}
