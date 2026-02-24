// @ts-nocheck
// InvalidDataSourceDefinition
import { DataSource } from "../../../src/ui/backend.ts";

@Model()
export class Foo {
  id: number;

  static readonly foo: DataSource<Foo> = {
    includeTree: 1,
  };
}
