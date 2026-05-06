import type { CloesceResult } from "./common.js";

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
  | "Env"
  | { Inject: { name: string } }
  | { Object: { name: string } }
  | { Partial: { object_name: string } }
  | { KvObject: CidlType }
  | { Paginated: CidlType }
  | { Nullable: CidlType }
  | { Array: CidlType }
  | { HttpResult: CidlType };

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
  | { OneToOne: { columns: string[] } }
  | { OneToMany: { columns: string[] } }
  | "ManyToMany";

export interface NavigationField {
  field: Field;
  model_reference: string;
  kind: NavigationFieldKind;
}

export interface KvField {
  field: ValidatedField;
  format: string;
  format_parameters: Field[];
  binding: string;
  list_prefix: boolean;
}

export interface R2Field {
  field: Field;
  format: string;
  format_parameters: Field[];
  binding: string;
  list_prefix: boolean;
}

export type MediaType = "Json" | "Octet";

export interface ApiMethod {
  name: string;
  is_static: boolean;
  http_verb: HttpVerb;
  return_media: MediaType;
  return_type: CidlType;
  parameters_media: MediaType;
  parameters: ValidatedField[];
  data_source: string | null;
}

export interface Model {
  name: string;
  d1_binding: string | null;
  primary_columns: Column[];
  columns: Column[];
  navigation_fields: NavigationField[];
  key_fields: ValidatedField[];
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

export interface Service {
  name: string;
  fields: Field[];
  apis: ApiMethod[];
}

export interface IncludeTree {
  [key: string]: IncludeTree;
}

export interface DataSourceListMethod {
  parameters: ValidatedField[];
}

export interface DataSourceGetMethodParam {
  parameter: ValidatedField;
  instance_field: boolean;
}

export interface DataSourceGetMethod {
  parameters: DataSourceGetMethodParam[];
}

export interface DataSourceImpl {
  include: IncludeTree;
  get: (env: any, ...args: unknown[]) => Promise<CloesceResult<unknown>>;
  list?: (env: any, ...args: unknown[]) => Promise<CloesceResult<unknown>>;
}

export interface DataSource {
  name: string;
  list?: DataSourceListMethod;
  get?: DataSourceGetMethod;
  is_internal: boolean;

  // Generated at runtime, not serialized.
  gen: DataSourceImpl;
}

export interface WranglerEnv {
  d1_bindings: string[];
  kv_bindings: string[];
  r2_bindings: string[];
  vars: Field[];
}

export interface Cidl {
  wrangler_env?: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  services: Record<string, Service>;
  injects: string[];
}

/** @internal */
export function getNavigationCidlType(nav: NavigationField): CidlType {
  return typeof nav.kind === "object" && "OneToOne" in nav.kind
    ? { Object: { name: nav.model_reference } }
    : { Array: { Object: { name: nav.model_reference } } };
}
