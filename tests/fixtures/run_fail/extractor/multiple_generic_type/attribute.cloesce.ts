// @ts-nocheck

@D1
export class Foo {
  @PrimaryKey
  id: number;
  bad: Bar<number, number>; // invalid type
}
