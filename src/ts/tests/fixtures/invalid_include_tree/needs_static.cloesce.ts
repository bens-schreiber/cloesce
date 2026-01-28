// @ts-nocheck
// InvalidDataSourceDefinition
import { IncludeTree } from "../../../src/ui/backend";

@Model()
export class Foo {
  id: number;

  bar: IncludeTree<Foo> = {}; // must be static
}
