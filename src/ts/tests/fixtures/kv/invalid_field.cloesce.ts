// @ts-nocheck
// InvalidKVModelField

class KVModel<T> {}

@KV("foo")
export class SomeKVModel extends KVModel<unknown> {
  invalidField: number;
}
