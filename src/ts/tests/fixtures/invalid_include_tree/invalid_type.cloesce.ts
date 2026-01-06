// @ts-nocheck
// InvalidDataSourceDefinition

@Model
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  static readonly bar: number = 1;
}
