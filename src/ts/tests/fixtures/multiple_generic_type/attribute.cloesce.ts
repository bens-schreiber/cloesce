// @ts-nocheck
// MultipleGenericType

@Model
export class Foo {
  @PrimaryKey
  id: number;
  bad: Bar<number, number>; // invalid type
}
