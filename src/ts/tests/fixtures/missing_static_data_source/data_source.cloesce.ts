// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  foo: IncludeTree<Foo> = {};
}
