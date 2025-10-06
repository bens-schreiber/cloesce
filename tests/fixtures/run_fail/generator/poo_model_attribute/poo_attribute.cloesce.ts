// @ts-nocheck

@PlainOldObject
class Poo {}

@WranglerEnv
class Env {}

@D1
class Foo {
  @PrimaryKey
  id: number;

  poo: Poo;
}
