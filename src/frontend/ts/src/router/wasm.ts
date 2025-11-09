import {
  CidlIncludeTree,
  CloesceAst,
  Either,
  Model,
  left,
  right,
} from "../common.js";
import { IncludeTree } from "../ui/backend.js";
import { RuntimeContainer } from "./router.js";

// Requires the ORM binary to have been built
import mod from "../orm.wasm";

/**
 * WASM ABI
 */
export interface OrmWasmExports {
  memory: WebAssembly.Memory;
  get_return_len(): number;
  get_return_ptr(): number;
  set_meta_ptr(ptr: number, len: number): number;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;

  object_relational_mapping(
    model_name_ptr: number,
    model_name_len: number,
    sql_rows_ptr: number,
    sql_rows_len: number,
    include_tree_ptr: number,
    include_tree_len: number,
  ): boolean;

  upsert_model(
    model_name_ptr: number,
    model_name_len: number,
    new_model_ptr: number,
    new_model_len: number,
    include_tree_ptr: number,
    include_tree_len: number,
  ): boolean;
}

/**
 * RAII for wasm memory
 */
export class WasmResource {
  constructor(
    private wasm: OrmWasmExports,
    public ptr: number,
    public len: number,
  ) {}
  free() {
    this.wasm.dealloc(this.ptr, this.len);
  }

  /**
   * Copies a value from TS memory to WASM memory. A subsequent `free` is necessary.
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
    throw Error("The WASM Module failed to load due to an invalid CIDL");
  }

  // Intentionally leak `modelMeta`, it should exist for the programs lifetime.
  return wasmInstance.exports;
}

export function invokeOrmWasm<T>(
  fn: (...args: number[]) => boolean,
  args: WasmResource[],
  wasm: OrmWasmExports,
): Either<string, T> {
  let resPtr: number | undefined;
  let resLen: number | undefined;

  try {
    const failed = fn(...args.flatMap((a) => [a.ptr, a.len]));
    resPtr = wasm.get_return_ptr();
    resLen = wasm.get_return_len();

    const result = new TextDecoder().decode(
      new Uint8Array(wasm.memory.buffer, resPtr, resLen),
    );

    return failed ? left(result) : right(result as T);
  } finally {
    args.forEach((a) => a.free());
    if (resPtr && resLen) wasm.dealloc(resPtr, resLen);
  }
}

/**
 * Calls `object_relational_mapping` to turn a row of SQL records into
 * an instantiated object.
 */
export function mapSql<T extends object>(
  ctor: new () => T,
  records: Record<string, any>[],
  includeTree: IncludeTree<T> | CidlIncludeTree | null,
): Either<string, T[]> {
  const { ast, constructorRegistry, wasm } = RuntimeContainer.get();
  const args = [
    WasmResource.fromString(ctor.name, wasm),
    WasmResource.fromString(JSON.stringify(records), wasm),
    WasmResource.fromString(JSON.stringify(includeTree), wasm),
  ];

  const jsonResults = invokeOrmWasm<string>(
    wasm.object_relational_mapping,
    args,
    wasm,
  );
  if (!jsonResults.ok) return jsonResults;

  const parsed: any[] = JSON.parse(jsonResults.value);
  return right(
    parsed.map((obj: any) =>
      instantiateDepthFirst(obj, ast.models[ctor.name], includeTree),
    ) as T[],
  );

  function instantiateDepthFirst(
    m: any,
    meta: Model,
    includeTree: IncludeTree<any> | null,
  ) {
    m = Object.assign(new constructorRegistry[meta.name](), m);

    if (!includeTree) {
      return m;
    }

    for (const navProp of meta.navigation_properties) {
      const nestedIncludeTree = includeTree[navProp.var_name];
      if (!nestedIncludeTree) continue;

      const nestedMeta = ast.models[navProp.model_name];
      const value = m[navProp.var_name];

      // One to Many, Many to Many
      if (Array.isArray(value)) {
        m[navProp.var_name] = value.map((child: any) =>
          instantiateDepthFirst(child, nestedMeta, nestedIncludeTree),
        );
      }
      // One to one
      else if (value) {
        m[navProp.var_name] = instantiateDepthFirst(
          value,
          nestedMeta,
          nestedIncludeTree,
        );
      }
    }

    return m;
  }
}
