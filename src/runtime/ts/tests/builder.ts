import {
  Model,
  Cidl,
  IncludeTree,
  Field,
  HttpVerb,
  CidlType,
  NavigationFieldKind,
  Column,
  DataSource,
  NavigationField,
  ApiMethod,
  Service,
  MediaType,
  KvR2Field,
  CrudKind,
} from "../src/cidl";

export function createAst(args?: {
  models?: Model[];
  services?: Service[];
}): Cidl {
  const modelsMap = Object.fromEntries(
    args?.models?.map((m) => [m.name, m]) ?? [],
  );
  const serviceMap = Object.fromEntries(
    args?.services?.map((s) => [s.name, s]) ?? [],
  );

  // NOTE: these won't always be empty in real usage
  for (const model of Object.values(modelsMap)) {
    model.data_sources["Default"] ??= {
      name: "Default",
      is_internal: false,
      gen: { include: {} },
    };
  }

  return {
    models: modelsMap,
    services: serviceMap,
    poos: {},
    wrangler_env: {
      d1_bindings: ["d1"],
      kv_bindings: [],
      r2_bindings: [],
      vars: [],
    },
  };
}

abstract class ApiMethodBuilder {
  protected apis: ApiMethod[] = [];

  method(
    name: string,
    http_verb: HttpVerb,
    is_static: boolean,
    parameters: Field[],
    return_type: CidlType,
    return_media: MediaType = "Json",
    parameters_media: MediaType = "Json",
    data_source: string | null = null,
  ): this {
    this.apis.push({
      name,
      http_verb,
      is_static,
      parameters,
      return_type,
      return_media,
      parameters_media,
      data_source,
    });
    return this;
  }
}

export class IncludeTreeBuilder {
  private nodes: IncludeTree = {};

  static new(): IncludeTreeBuilder {
    return new IncludeTreeBuilder();
  }

  addNode(name: string): this {
    this.nodes[name] = {};
    return this;
  }

  addWithChildren(
    name: string,
    build: (b: IncludeTreeBuilder) => IncludeTreeBuilder,
  ): this {
    const subtree = build(new IncludeTreeBuilder()).build();
    this.nodes[name] = subtree;
    return this;
  }

  build(): IncludeTree {
    return this.nodes;
  }
}

export class ModelBuilder {
  private name: string;
  private d1_binding: string | null = null;
  private primary_key_names: string[] = [];
  private primary_key_types: Record<string, CidlType> = {};
  private columns: Column[] = [];
  private navigation_fields: NavigationField[] = [];
  private key_fields: string[] = [];
  private kv_fields: KvR2Field[] = [];
  private r2_fields: KvR2Field[] = [];
  private apis: ApiMethod[] = [];
  private data_sources: Record<string, DataSource> = {};
  private cruds: CrudKind[] = [];

  constructor(name: string) {
    this.name = name;
  }

  static model(name: string): ModelBuilder {
    return new ModelBuilder(name);
  }

  d1(binding: string): this {
    this.d1_binding = binding;
    return this;
  }

  defaultDb(): this {
    this.d1_binding = "d1";
    return this;
  }

  col(
    name: string,
    cidl_type: CidlType,
    foreign_key: { model_name: string; column_name: string } | null = null,
  ): this {
    this.columns.push({
      field: { name, cidl_type },
      foreign_key_reference: foreign_key,
      unique_ids: [],
      composite_id: null,
    });
    return this;
  }

  navP(name: string, model_reference: string, kind: NavigationFieldKind): this {
    this.navigation_fields.push({
      field: { name, cidl_type: { Object: { name: model_reference } } },
      model_reference,
      kind,
    });
    return this;
  }

  pk(name: string, cidl_type: CidlType): this {
    if (!this.primary_key_names.includes(name)) {
      this.primary_key_names.push(name);
    }
    this.primary_key_types[name] = cidl_type;
    return this;
  }

  idPk(): this {
    return this.pk("id", "Integer");
  }

  keyField(name: string): this {
    this.key_fields.push(name);
    return this;
  }

  kvField(
    format: string,
    binding: string,
    name: string,
    list_prefix: boolean,
    cidl_type: CidlType,
  ): this {
    this.kv_fields.push({
      field: { name, cidl_type },
      format,
      binding,
      list_prefix,
    });
    return this;
  }

  r2Field(
    format: string,
    binding: string,
    name: string,
    list_prefix: boolean,
    cidl_type: CidlType = "R2Object",
  ): this {
    this.r2_fields.push({
      field: { name, cidl_type },
      format,
      binding,
      list_prefix,
    });
    return this;
  }

  method(
    name: string,
    http_verb: HttpVerb,
    parameters: Field[],
    return_type: CidlType,
    data_source: string | null = null,
  ): this {
    this.apis.push({
      name,
      http_verb,
      is_static: data_source === null,
      parameters,
      return_type,
      return_media: "Json",
      parameters_media: "Json",
      data_source,
    });
    return this;
  }

  dataSource(
    name: string,
    tree: IncludeTree,
    get?: Field[],
    is_internal: boolean = false,
  ): this {
    this.data_sources[name] = {
      name,
      is_internal,
      gen: { include: tree },
      get: get ? { parameters: get } : undefined,
    };
    return this;
  }

  crud(kind: CrudKind): this {
    this.cruds.push(kind);
    return this;
  }

  build(): Model {
    const mutableColumns = [...this.columns];
    const primary_columns: Column[] = [];

    for (const pkName of this.primary_key_names) {
      const idx = mutableColumns.findIndex((col) => col.field.name === pkName);
      if (idx >= 0) {
        primary_columns.push(mutableColumns[idx]);
        mutableColumns.splice(idx, 1);
      } else {
        primary_columns.push({
          field: {
            name: pkName,
            cidl_type: this.primary_key_types[pkName] ?? "Integer",
          },
          foreign_key_reference: null,
          unique_ids: [],
          composite_id: null,
        });
      }
    }

    return {
      name: this.name,
      d1_binding: this.d1_binding,
      primary_columns,
      columns: mutableColumns,
      navigation_fields: this.navigation_fields,
      key_fields: this.key_fields,
      kv_fields: this.kv_fields,
      r2_fields: this.r2_fields,
      apis: this.apis,
      data_sources: this.data_sources,
      cruds: this.cruds,
    };
  }
}

export class ServiceBuilder extends ApiMethodBuilder {
  private name: string;
  private fields: Field[] = [];

  constructor(name: string) {
    super();
    this.name = name;
  }

  static service(name: string) {
    return new ServiceBuilder(name);
  }

  field(name: string, cidl_type: CidlType): this {
    this.fields.push({ name, cidl_type });
    return this;
  }

  build(): Service {
    return {
      name: this.name,
      fields: this.fields,
      apis: this.apis,
    };
  }
}
