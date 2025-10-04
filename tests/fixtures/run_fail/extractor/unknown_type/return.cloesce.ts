// @ts-nocheck

@D1
class Foo {
  @PrimaryKey
  id: number;

  async method(valid: number): Bar<number> {} // invalid return
}
