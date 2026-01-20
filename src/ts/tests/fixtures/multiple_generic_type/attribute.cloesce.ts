// @ts-nocheck
// MultipleGenericType

@Model
export class Foo {
  id: number;
  bad: Bar<number, number>; // invalid type
}
