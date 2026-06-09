/** NOTE: These definitions mirror the definitions in the Compiler */
export type CrudKind = "Save" | "Get" | "List";

export type CidlType =
  | "Void"
  | "Int"
  | "Real"
  | "String"
  | "Blob"
  | "DateIso"
  | "Boolean"
  | "Stream"
  | "Json"
  | "R2Object"
  | { Object: { name: string } }
  | { Partial: { object_name: string } }
  | { KvObject: CidlType }
  | { Paginated: CidlType }
  | { Nullable: CidlType }
  | { Array: CidlType };

export type HttpVerb = "Get" | "Post" | "Put" | "Patch" | "Delete";

export type Number = { Int: number } | { Float: number };

export interface Field {
  name: string;
  cidl_type: CidlType;
}

export interface ValidatedField {
  name: string;
  cidl_type: CidlType;

  // No type because it is not valuable
  // to the TypeScript side of things, just the ORM.
  validators: unknown[];
}

export interface ForeignKeyReference {
  model_name: string;
  column_name: string;
}

export interface Column {
  field: ValidatedField;
  foreign_key_reference: ForeignKeyReference | null;
  unique_ids: number[];
  composite_id: number | null;
}

export type NavigationFieldKind =
  | { OneToOne: { fields: string[] } }
  | { OneToMany: { columns: string[] } };

export interface NavigationField {
  field: Field;
  model_reference: string;
  kind: NavigationFieldKind;
}

export interface KvField {
  field: ValidatedField;
  binding: string;
  key_format: string;
}

export interface R2Field {
  field: Field;
  binding: string;
  key_format: string;
}

export type MediaType = "Json" | "Octet";

/**
 * Durable Object context is implicitly injected to API methods under
 * this key.
 */
export const CONTEXT_INJECT_KEY = "$ctx";

export interface DurableTarget {
  binding: string;
  shard_args: string[];
}

export interface ApiMethod {
  name: string;
  is_static: boolean;
  http_verb: HttpVerb;
  return_media: MediaType;
  return_type: CidlType;
  parameters_media: MediaType;
  parameters: ValidatedField[];
  data_source: string | null;
  injected: string[];
  durable_target?: DurableTarget | null;
}

export interface Model {
  name: string;
  database_binding: string | null;
  primary_columns: Column[];
  columns: Column[];
  navigation_fields: NavigationField[];
  route_fields: ValidatedField[];
  kv_fields: KvField[];
  r2_fields: R2Field[];
  apis: ApiMethod[];
  cruds: CrudKind[];
  data_sources: Record<string, DataSource>;
}

export interface PlainOldObject {
  name: string;
  fields: ValidatedField[];
}

export interface IncludeTree {
  [key: string]: IncludeTree;
}

export interface DataSourceMethod {
  parameters: ValidatedField[];
  injected: string[];
  is_stub: boolean;
}

export interface DataSourceGetMethodParam {
  parameter: ValidatedField;
  instance_field: boolean;
}

export interface DataSourceGetMethod {
  parameters: DataSourceGetMethodParam[];
  injected: string[];
  is_stub: boolean;
}

export interface DataSource {
  name: string;
  tree: IncludeTree;
  include_query: string;
  get_query: string;
  list_query: string;
  get: DataSourceGetMethod;
  list: DataSourceMethod;
  save: DataSourceMethod;
  is_internal: boolean;
}

export interface WranglerEnv {
  d1_bindings: string[];
  kv_bindings: unknown[];
  r2_bindings: unknown[];
  durable_bindings: unknown[];
  vars: Field[];
}

export interface Cidl {
  wrangler_env?: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  injects: string[];
}

/** @internal */
export function getNavigationCidlType(nav: NavigationField): CidlType {
  return typeof nav.kind === "object" && "OneToOne" in nav.kind
    ? { Object: { name: nav.model_reference } }
    : { Array: { Object: { name: nav.model_reference } } };
}
