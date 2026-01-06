// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Bar {
  @PrimaryKey
  id: number;
}

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly default: IncludeTree<Bar> = {};
}
