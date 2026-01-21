// @ts-nocheck
// InvalidDataSourceDefinition

@Model()
export class Foo {
  id: number;

  bar: IncludeTree<Foo> = {}; // must be static
}
