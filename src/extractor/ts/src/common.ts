export type Either<L, R> = { ok: false; value: L } | { ok: true; value: R };
export function left<L>(value: L): Either<L, never> {
  return { ok: false, value };
}
export function right<R>(value: R): Either<never, R> {
  return { ok: true, value };
}

export type HttpResult<T = unknown> = {
  ok: boolean;
  status: number;
  data?: T;
  message?: string;
};

export type CidlType =
  | "Void"
  | "Integer"
  | "Real"
  | "Text"
  | "Blob"
  | { Inject: string }
  | { Model: string }
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
  var_name: string;
  model_name: string;
  kind: NavigationPropertyKind;
}

export function getNavigationPropertyCidlType(
  nav: NavigationProperty,
): CidlType {
  return "OneToOne" in nav.kind
    ? { Model: nav.model_name }
    : { Array: { Model: nav.model_name } };
}

export interface Model {
  name: string;
  primary_key: NamedTypedValue;
  attributes: ModelAttribute[];
  navigation_properties: NavigationProperty[];
  methods: Record<string, ModelMethod>;
  data_sources: Record<string, DataSource>;
  source_path: string;
}

export interface CidlIncludeTree {
  [key: string]: CidlIncludeTree;
}

export interface DataSource {
  name: string;
  tree: CidlIncludeTree;
}

export interface WranglerEnv {
  name: string;
  source_path: string;
}

export interface CloesceAst {
  version: string;
  project_name: string;
  language: "TypeScript";
  wrangler_env: WranglerEnv;
  models: Record<string, Model>;
}
