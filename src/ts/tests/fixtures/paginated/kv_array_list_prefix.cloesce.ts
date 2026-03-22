// @ts-nocheck
// MissingKValue

import { KValue } from "../../../src/ui/backend";

@Model()
export class Foo {
  id: number;

  @KV("value/Foo", "namespace")
  allValues: KValue<unknown>[];
}
