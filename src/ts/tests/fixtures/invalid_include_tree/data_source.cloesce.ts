// @ts-nocheck
// InvalidDataSourceDefinition
import { IncludeTree } from "../../../src/ui/backend";

@Model()
export class Foo {
  id: number;

  foo: IncludeTree<Foo> = {};
}
