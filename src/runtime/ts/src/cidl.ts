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

export type NavigationCardinality = "One" | "Many";

export interface NavigationKeyMapping {
  local: string;
  target: string;
}

export interface NavigationField {
  field: Field;
  model_reference: string;
  target_backing?: ModelBacking | null;
  cardinality: NavigationCardinality;
  keys: NavigationKeyMapping[];
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
 * Methods that run inside a Durable Object's context receive that DO instance
 * in their `env` under this key.
 */
export const ENV_DURABLE_TARGET_KEY = "ctx";

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

export type BackingKind = "D1" | "DurableObject";

export interface ModelBacking {
  binding: string;
  fields: string[];
  kind: BackingKind;
}

export interface Model {
  name: string;
  backing?: ModelBacking | null;
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

export function isDurableBacked(model: Model): boolean {
  return model.backing?.kind === "DurableObject";
}

export function isD1Backed(model: Model): boolean {
  return model.backing?.kind === "D1";
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
  durable_target?: DurableTarget | null;
}

export interface DataSourceGetMethodParam {
  parameter: ValidatedField;
  instance_field: boolean;
}

export interface DataSourceGetMethod {
  parameters: DataSourceGetMethodParam[];
  injected: string[];
  is_stub: boolean;
  durable_target?: DurableTarget | null;
}

export interface DataSource {
  name: string;
  tree: IncludeTree;
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
  return nav.cardinality === "One"
    ? { Object: { name: nav.model_reference } }
    : { Array: { Object: { name: nav.model_reference } } };
}
