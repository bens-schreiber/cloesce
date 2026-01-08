// @ts-nocheck
// UnknownType

@Model
export class Foo {
  @PrimaryKey
  id: number;
  bad: Bar<number>; // invalid type
}
