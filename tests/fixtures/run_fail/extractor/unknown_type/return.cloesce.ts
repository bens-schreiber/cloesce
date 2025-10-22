// @ts-nocheck

@D1
export class Foo {
  @PrimaryKey
  id: number;

  method(valid: number): Bar<number> {} // invalid return
}
