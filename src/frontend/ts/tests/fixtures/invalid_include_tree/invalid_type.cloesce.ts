// @ts-nocheck
// InvalidDataSourceDefinition

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly bar: number = 1;
}
