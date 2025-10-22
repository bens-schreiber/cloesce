// @ts-nocheck

@D1
export class Foo {
  @PrimaryKey
  id: number;

  @DataSource
  bar: number; // invalid data source type
}
