// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  bar: IncludeTree<Foo> = {}; // must be static
}
