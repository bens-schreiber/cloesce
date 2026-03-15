/** NOTE: These definitions mirror the definitions in the Generator */

/**
 * Kinds of CRUD operations supported for a model.
 *
 * - "SAVE": Create or update an entity.
 * - "GET": Retrieve a single entity by its primary key.
 * - "LIST": Retrieve a list of entities.
 */
export type CrudKind = "SAVE" | "GET" | "LIST";

export type CidlType =
  | "Void"
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | "DateIso"
  | "Boolean"
  | "Stream"
  | "JsonValue"
  | "R2Object"
  | { DataSource: string }
  | { Inject: string }
  | { Object: string }
  | { Partial: string }
  | { KvObject: CidlType }
  | { Paginated: CidlType }
  | { Nullable: CidlType }
  | { Array: CidlType }
  | { HttpResult: CidlType };

/** @internal */
export function isNullableType(ty: CidlType): boolean {
  return typeof ty === "object" && ty !== null && "Nullable" in ty;
}

export enum HttpVerb {
  Get = "Get",
  Post = "Post",
  Put = "Put",
  Patch = "Patch",
  Delete = "Delete",
}

export interface NamedTypedValue {
  name: string;
  cidl_type: CidlType;
}

export interface ForeignKeyReference {
  model_name: string;
  column_name: string;
}

export interface D1Column {
  value: NamedTypedValue;
  foreign_key_reference: ForeignKeyReference | null;
  unique_ids: number[];
  composite_id: number | null;
}

export enum MediaType {
  Json = "Json",
  Octet = "Octet",
}

/**
 * @internal
 * A placeholder value which should be updated by the generator.
 *
 * @returns MediaType.Json
 */
export function defaultMediaType(): MediaType {
  return MediaType.Json;
}

export interface ApiMethod {
  name: string;
  is_static: boolean;
  data_source: string | null;
  http_verb: HttpVerb;

  return_media: MediaType;
  return_type: CidlType;

  parameters_media: MediaType;
  parameters: NamedTypedValue[];
}

export type NavigationPropertyKind =
  | { OneToOne: { key_columns: string[] } }
  | { OneToMany: { key_columns: string[] } }
  | "ManyToMany";

export interface NavigationProperty {
  var_name: string;
  model_reference: string;
  kind: NavigationPropertyKind;
}

/** @internal */
export function getNavigationPropertyCidlType(
  nav: NavigationProperty,
): CidlType {
  return typeof nav.kind !== "string" && "OneToOne" in nav.kind
    ? { Object: nav.model_reference }
    : { Array: { Object: nav.model_reference } };
}

export interface KeyValue {
  format: string;
  namespace_binding: string;
  value: NamedTypedValue;
  list_prefix: boolean;
}

export interface AstR2Object {
  format: string;
  bucket_binding: string;
  var_name: string;
  list_prefix: boolean;
}

export interface Model {
  name: string;
  d1_binding: string | null;
  primary_key_columns: D1Column[];
  columns: D1Column[];
  navigation_properties: NavigationProperty[];
  key_params: string[];
  kv_objects: KeyValue[];
  r2_objects: AstR2Object[];
  methods: Record<string, ApiMethod>;
  data_sources: Record<string, DataSource>;
  cruds: CrudKind[];
  source_path: string;
}

export interface PlainOldObject {
  name: string;
  attributes: NamedTypedValue[];
  source_path: string;
}

export interface ServiceAttribute {
  var_name: string;
  inject_reference: string;
}

export interface Service {
  name: string;
  attributes: ServiceAttribute[];
  methods: Record<string, ApiMethod>;
  source_path: string;
  initializer: string[] | null;
}

export interface CidlIncludeTree {
  [key: string]: CidlIncludeTree;
}

export type CrudListParam = "LastSeen" | "Limit" | "Offset";

export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
  is_private: boolean;
  list_params: CrudListParam[];
}

export interface WranglerEnv {
  name: string;
  source_path: string;
  d1_bindings: string[];
  kv_bindings: string[];
  r2_bindings: string[];
  vars: Record<string, CidlType>;
}

export interface CloesceAst {
  project_name: string;
  wrangler_env?: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  services: Record<string, Service>;
  main_source: string | null;
}
