// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  id: number;

  foo: IncludeTree<Foo> = { bar: {} }; // bar does not exist
}
