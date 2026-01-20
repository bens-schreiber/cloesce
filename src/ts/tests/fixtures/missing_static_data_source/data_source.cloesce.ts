// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  @PrimaryKey
  id: number;

  foo: IncludeTree<Foo> = {};
}
