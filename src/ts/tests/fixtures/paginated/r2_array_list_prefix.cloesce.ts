// @ts-nocheck
// MissingR2ObjectBody

import { R2ObjectBody } from "../../../src/ui/backend";

@Model()
export class Foo {
  id: number;

  @R2("files/Foo", "bucket")
  allFiles: R2ObjectBody[];
}
