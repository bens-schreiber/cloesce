// @ts-nocheck

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  bar: IncludeTree<Foo> = {}; // must be static
}
