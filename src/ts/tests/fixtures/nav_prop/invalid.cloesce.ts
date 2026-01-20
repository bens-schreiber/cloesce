// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  @PrimaryKey
  id: number;

  foo: IncludeTree<Foo> = { id: {} }; // id is not a model
}
