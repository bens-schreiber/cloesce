// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  @PrimaryKey
  id: number;

  bar: IncludeTree<Foo> = {}; // must be static
}
