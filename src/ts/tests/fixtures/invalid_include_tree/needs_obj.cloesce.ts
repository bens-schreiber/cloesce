// @ts-nocheck
// InvalidDataSourceDefinition
import { DataSource } from "../../../src/ui/backend.ts";

@Model()
export class Foo {
  id: number;

  // lacks object initializer
  static readonly foo: DataSource<Foo> = 1;
}
