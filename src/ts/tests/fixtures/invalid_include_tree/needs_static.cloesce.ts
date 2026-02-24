// @ts-nocheck
// InvalidDataSourceDefinition
import { DataSource } from "../../../src/ui/backend.ts";

@Model()
export class Foo {
  id: number;

  // lacks `static readonly`
  foo: DataSource<Foo> = {};
}
