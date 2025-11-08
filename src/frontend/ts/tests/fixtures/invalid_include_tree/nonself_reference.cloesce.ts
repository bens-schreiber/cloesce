// @ts-nocheck
// InvalidDataSourceDefinition

@D1
export class Bar {
  @PrimaryKey
  id: number;
}

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly default: IncludeTree<Bar> = {};
}
