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
  | { DataSource: string }
  | { Inject: string }
  | { Object: string }
  | { Partial: string }
  | { Nullable: CidlType }
  | { Array: CidlType }
  | { HttpResult: CidlType };

export function isNullableType(ty: CidlType): boolean {
  return typeof ty === "object" && ty !== null && "Nullable" in ty;
}

export enum HttpVerb {
  GET = "GET",
  POST = "POST",
  PUT = "PUT",
  PATCH = "PATCH",
  DELETE = "DELETE",
}

export interface NamedTypedValue {
  name: string;
  cidl_type: CidlType;
}

export interface D1ModelAttribute {
  value: NamedTypedValue;
  foreign_key_reference: string | null;
}

export enum MediaType {
  Json = "Json",
  Octet = "Octet",
}

/**
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
  http_verb: HttpVerb;

  return_media: MediaType;
  return_type: CidlType;

  parameters_media: MediaType;
  parameters: NamedTypedValue[];
}

export type NavigationPropertyKind =
  | { OneToOne: { attribute_reference: string } }
  | { OneToMany: { attribute_reference: string } }
  | { ManyToMany: { unique_id: string } };

export interface D1NavigationProperty {
  var_name: string;
  model_reference: string;
  kind: NavigationPropertyKind;
}

export function getNavigationPropertyCidlType(
  nav: D1NavigationProperty,
): CidlType {
  return "OneToOne" in nav.kind
    ? { Object: nav.model_reference }
    : { Array: { Object: nav.model_reference } };
}

export interface D1Model {
  name: string;
  primary_key: NamedTypedValue;
  attributes: D1ModelAttribute[];
  navigation_properties: D1NavigationProperty[];
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

export type KVNavigationProperty =
  | { KValue: NamedTypedValue }
  | {
      Model: {
        model_reference: string;
        var_name: string;
        many: boolean;
      };
    };

export interface KVModel {
  name: string;
  binding: string;
  cidl_type: CidlType;
  params: string[];
  navigation_properties: KVNavigationProperty[];
  cruds: CrudKind[];
  methods: Record<string, ApiMethod>;
  data_sources: Record<string, DataSource>;
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
}

export interface CidlIncludeTree {
  [key: string]: CidlIncludeTree;
}

export const NO_DATA_SOURCE = "none";
export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

export interface WranglerEnv {
  name: string;
  source_path: string;
  d1_binding?: string; // TODO: multiple D1 bindings
  kv_bindings: string[];
  vars: Record<string, CidlType>;
}

export interface CloesceAst {
  project_name: string;
  wrangler_env?: WranglerEnv;
  d1_models: Record<string, D1Model>;
  kv_models: Record<string, KVModel>;
  poos: Record<string, PlainOldObject>;
  services: Record<string, Service>;
  app_source: string | null;
}
