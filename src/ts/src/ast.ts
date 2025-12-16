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

export interface ModelAttribute {
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
  | { OneToOne: { reference: string } }
  | { OneToMany: { reference: string } }
  | { ManyToMany: { unique_id: string } };

export interface NavigationProperty {
  var_name: string;
  model_name: string;
  kind: NavigationPropertyKind;
}

export function getNavigationPropertyCidlType(
  nav: NavigationProperty,
): CidlType {
  return "OneToOne" in nav.kind
    ? { Object: nav.model_name }
    : { Array: { Object: nav.model_name } };
}

export interface Model {
  name: string;
  primary_key: NamedTypedValue;
  attributes: ModelAttribute[];
  navigation_properties: NavigationProperty[];
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
  injected: string;
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
  db_binding: string;
  vars: Record<string, CidlType>;
}

export interface CloesceAst {
  [x: string]: any;
  version: string;
  project_name: string;
  language: string;
  wrangler_env?: WranglerEnv;
  models: Record<string, Model>;
  poos: Record<string, PlainOldObject>;
  services: Record<string, Service>;
  app_source: string | null;
}
