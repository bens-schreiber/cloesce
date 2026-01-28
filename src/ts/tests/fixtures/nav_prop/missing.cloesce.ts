// @ts-nocheck
// InvalidSelectorSyntax
class Bar {}

@Model()
export class Foo {
  id: number;

  @OneToOne() // missing selector
  bar: Bar;
}
