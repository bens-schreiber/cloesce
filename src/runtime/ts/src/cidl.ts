/** NOTE: These definitions mirror the definitions in the Compiler */

export type CrudKind = "Save" | "Get" | "List";

export type CidlType =
  | "Void"
  | "Integer"
  | "Double"
  | "String"
  | "Blob"
  | "DateIso"
  | "Boolean"
  | "Stream"
  | "Json"
  | "R2Object"
  | "Env"
  | { DataSource: { model_name: string } }
  | { Inject: { name: string } }
  | { Object: { name: string } }
  | { Partial: { object_name: string } }
  | { KvObject: CidlType }
  | { Paginated: CidlType }
  | { Nullable: CidlType }
  | { Array: CidlType }
  | { HttpResult: CidlType };

export type HttpVerb = "Get" | "Post" | "Put" | "Patch" | "Delete";

export interface Field {
  name: string;
  cidl_type: CidlType;
}

export interface ForeignKeyReference {
  model_name: string;
  column_name: string;
}

export interface Column {
  field: Field;
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

export interface KvR2Field {
  field: Field;
  format: string;
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
  parameters: Field[];
  data_source: string | null;
}

export interface Model {
  name: string;
  d1_binding: string | null;
  primary_columns: Column[];
  columns: Column[];
  navigation_fields: NavigationField[];
  key_fields: string[];
  kv_fields: KvR2Field[];
  r2_fields: KvR2Field[];
  apis: ApiMethod[];
  cruds: CrudKind[];
  data_sources: Record<string, DataSource>;
}

export interface PlainOldObject {
  name: string;
  fields: Field[];
}

export interface Service {
  name: string;
  fields: Field[];
  apis: ApiMethod[];
}

export interface IncludeTree {
  [key: string]: IncludeTree;
}

export interface DataSourceMethod {
  parameters: Field[];
}

export interface DataSourceImpl {
  include: IncludeTree;
  get: (env: any, ...args: unknown[]) => unknown;
  list?: (env: any, ...args: unknown[]) => unknown;
}

export interface DataSource {
  name: string;
  list?: DataSourceMethod;
  get?: DataSourceMethod;
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
}

/** @internal */
export function getNavigationCidlType(nav: NavigationField): CidlType {
  return typeof nav.kind === "object" && "OneToOne" in nav.kind
    ? { Object: { name: nav.model_reference } }
    : { Array: { Object: { name: nav.model_reference } } };
}
