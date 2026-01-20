// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  id: number;

  foo: IncludeTree<Foo> = { id: {} }; // id is not a model
}
