// @ts-nocheck
// UnknownType

@Model()
export class Foo {
  id: number;
  bad: Bar<number>; // invalid type
}
