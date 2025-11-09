// @ts-nocheck
// InvalidDataSourceDefinition

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  foo: IncludeTree<Foo> = {};
}
