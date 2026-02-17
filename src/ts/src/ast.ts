/** NOTE: These definitions mirror the definitions in the Generator */

/**
 * Kinds of CRUD operations supported for a model.
 *
 * - "SAVE": Create or update an entity.
 * - "GET": Retrieve a single entity by its primary key.
 * - "LIST": Retrieve a list of entities.
 */
export type CrudKind = "SAVE" | "GET" | "LIST";

/** @internal */
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
  | { Nullable: CidlType }
  | { Array: CidlType }
  | { HttpResult: CidlType };

/** @internal */
export function isNullableType(ty: CidlType): boolean {
  return typeof ty === "object" && ty !== null && "Nullable" in ty;
}

/** @internal */
export enum HttpVerb {
  GET = "GET",
  POST = "POST",
  PUT = "PUT",
  PATCH = "PATCH",
  DELETE = "DELETE",
}

/** @internal */
export interface NamedTypedValue {
  name: string;
  cidl_type: CidlType;
}

/** @internal */
export interface D1Column {
  value: NamedTypedValue;
  foreign_key_reference: string | null;
}

/** @internal */
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

/** @internal */
export interface ApiMethod {
  name: string;
  is_static: boolean;
  http_verb: HttpVerb;

  return_media: MediaType;
  return_type: CidlType;

  parameters_media: MediaType;
  parameters: NamedTypedValue[];
}

/** @internal */
export type NavigationPropertyKind =
  | { OneToOne: { column_reference: string } }
  | { OneToMany: { column_reference: string } }
  | "ManyToMany";

/** @internal */
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

/** @internal */
export interface KeyValue {
  format: string;
  namespace_binding: string;
  value: NamedTypedValue;
  list_prefix: boolean;
}

/** @internal */
export interface AstR2Object {
  format: string;
  bucket_binding: string;
  var_name: string;
  list_prefix: boolean;
}

/** @internal */
export interface Model {
  name: string;
  primary_key: NamedTypedValue | null;
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

/** @internal */
export interface PlainOldObject {
  name: string;
  attributes: NamedTypedValue[];
  source_path: string;
}

/** @internal */
export interface ServiceAttribute {
  var_name: string;
  inject_reference: string;
}

/** @internal */
export interface Service {
  name: string;
  attributes: ServiceAttribute[];
  methods: Record<string, ApiMethod>;
  source_path: string;
  initializer: string[] | null;
}

/** @internal */
export interface CidlIncludeTree {
  [key: string]: CidlIncludeTree;
}

/** @internal */
export const NO_DATA_SOURCE = "none";

/** @internal */
export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

/** @internal */
export interface WranglerEnv {
  name: string;
  source_path: string;
  d1_binding?: string; // TODO: multiple D1 bindings
  kv_bindings: string[];
  r2_bindings: string[];
  vars: Record<string, CidlType>;
}

/** @internal */
export interface CloesceAst {
  project_name: string;
  wrangler_env?: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  services: Record<string, Service>;
  main_source: string | null;
}
