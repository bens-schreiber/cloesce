import { Named } from "cmd-ts/dist/cjs/helpdoc.js";

export type Either<L, R> = { ok: false; value: L } | { ok: true; value: R };
export function left<L>(value: L): Either<L, never> {
  return { ok: false, value };
}
export function right<R>(value: R): Either<never, R> {
  return { ok: true, value };
}

/**
 * A `Model` meant for Cloesce Meta Data, utilzing an map of methods.
 */
export interface MetaModel {
  name: string;
  attributes: ModelAttribute[];
  primary_key: NamedTypedValue;
  navigation_properties: NavigationProperty[];
  data_sources: DataSource[];
  methods: Record<string, ModelMethod>;
}

/**
 * A `Cidl` meant for Cloesce Meta Data, utilzing an map of models.
 */
export type MetaCidl = {
  wrangler_env: WranglerEnv;
  models: Record<string, MetaModel>;
  [key: string]: unknown;
};

// --------------------------------------------------
// V CIDL types, mirroring the Rust bindings V
// --------------------------------------------------

export type HttpResult<T = unknown> = {
  ok: boolean;
  status: number;
  data?: T;
  message?: string;
};

export type CidlType =
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | "D1Database"
  | { Inject: string }
  | { Model: string }
  | { Array: CidlType }
  | { HttpResult: CidlType | null };

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
  nullable: boolean;
}

export interface ModelAttribute {
  value: NamedTypedValue;
  foreign_key_reference: string | null;
}

export interface ModelMethod {
  name: string;
  is_static: boolean;
  http_verb: HttpVerb;
  return_type: CidlType | null;
  parameters: NamedTypedValue[];
}

export type NavigationPropertyKind =
  | { OneToOne: { reference: string } }
  | { OneToMany: { reference: string } }
  | { ManyToMany: { unique_id: string } };

export interface NavigationProperty {
  value: NamedTypedValue;
  kind: NavigationPropertyKind;
}

export interface Model {
  name: string;
  attributes: ModelAttribute[];
  primary_key: NamedTypedValue;
  navigation_properties: NavigationProperty[];
  data_sources: DataSource[];
  methods: ModelMethod[];
  source_path: string;
}

/**
 * The CIDL or JSON IncludeTree structure
 */
export type CidlIncludeTree = Array<[NamedTypedValue, CidlIncludeTree]>;

export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

export interface WranglerEnv {
  name: string;
  source_path: string;
}

export interface CidlSpec {
  version: string;
  project_name: string;
  language: "TypeScript";
  wrangler_env: WranglerEnv;
  models: Model[];
}
