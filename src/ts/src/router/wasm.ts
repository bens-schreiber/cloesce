import { CloesceAst } from "../ast.js";
import { Either } from "../common.js";

// NOTE: Requires the ORM binary to have been built
import mod from "../orm.wasm";

/**
 * Cloesce WASM ABI
 */
export interface OrmWasmExports {
  memory: WebAssembly.Memory;
  get_return_len(): number;
  get_return_ptr(): number;
  set_meta_ptr(ptr: number, len: number): number;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;

  upsert_model(
    model_name_ptr: number,
    model_name_len: number,
    new_model_ptr: number,
    new_model_len: number,
    include_tree_ptr: number,
    include_tree_len: number,
  ): boolean;

  as_json(
    model_name_ptr: number,
    model_name_len: number,
    include_tree_ptr: number,
    include_tree_len: number,
  ): boolean;
}

export class WasmResource {
  private constructor(
    private wasm: OrmWasmExports,
    public ptr: number,
    public len: number,
  ) {}

  free() {
    this.wasm.dealloc(this.ptr, this.len);
  }

  /**
   * Copies a value from TS memory to WASM memory.
   *
   * A subsequent call to `free` is necessary.
   */
  static fromString(str: string, wasm: OrmWasmExports): WasmResource {
    const encoder = new TextEncoder();
    const bytes = encoder.encode(str);
    const ptr = wasm.alloc(bytes.length);
    const mem = new Uint8Array(wasm.memory.buffer, ptr, bytes.length);
    mem.set(bytes);
    return new this(wasm, ptr, bytes.length);
  }
}

export async function loadOrmWasm(
  ast: CloesceAst,
  wasm?: WebAssembly.Instance,
): Promise<OrmWasmExports> {
  // Load WASM
  const wasmInstance = (wasm ??
    (await WebAssembly.instantiate(mod))) as WebAssembly.Instance & {
    exports: OrmWasmExports;
  };

  const modelMeta = WasmResource.fromString(
    JSON.stringify(ast.models),
    wasmInstance.exports,
  );

  if (wasmInstance.exports.set_meta_ptr(modelMeta.ptr, modelMeta.len) != 0) {
    modelMeta.free();
    const resPtr = wasmInstance.exports.get_return_ptr();
    const resLen = wasmInstance.exports.get_return_len();
    const errorMsg = new TextDecoder().decode(
      new Uint8Array(wasmInstance.exports.memory.buffer, resPtr, resLen),
    );

    throw Error(
      `"The WASM Module failed to load due to an invalid CIDL: ${errorMsg}`,
    );
  }

  // Intentionally leak `modelMeta`, it should exist for the programs lifetime.
  return wasmInstance.exports;
}

/**
 * Invokes a WASM ORM function with the provided arguments, handling memory
 * allocation and deallocation.
 *
 * Returns an Either where Left is an error message and Right the raw string result.
 */
export function invokeOrmWasm(
  fn: (...args: number[]) => boolean,
  args: WasmResource[],
  wasm: OrmWasmExports,
): Either<string, string> {
  let resPtr: number | undefined;
  let resLen: number | undefined;

  try {
    const failed = fn(...args.flatMap((a) => [a.ptr, a.len]));
    resPtr = wasm.get_return_ptr();
    resLen = wasm.get_return_len();

    const result = new TextDecoder().decode(
      new Uint8Array(wasm.memory.buffer, resPtr, resLen),
    );

    return failed ? Either.left(result) : Either.right(result);
  } finally {
    args.forEach((a) => a.free());
    if (resPtr && resLen) wasm.dealloc(resPtr, resLen);
  }
}
