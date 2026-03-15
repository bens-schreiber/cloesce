import { Project } from "ts-morph";
import {
  Model,
  CloesceAst,
  CidlIncludeTree,
  NamedTypedValue,
  HttpVerb,
  CidlType,
  NavigationPropertyKind,
  D1Column,
  DataSource,
  NavigationProperty,
  ApiMethod,
  Service,
  ServiceAttribute,
  MediaType,
  KeyValue,
  AstR2Object,
  CrudKind,
  CrudListParam,
} from "../src/ast";

export function cloesceProject(): Project {
  const project = new Project({
    compilerOptions: {
      strict: true,
    },
  });

  project.addSourceFileAtPath("./src/ui/backend.ts");
  return project;
}

export function createAst(args?: {
  models?: Model[];
  services?: Service[];
}): CloesceAst {
  const modelsMap = Object.fromEntries(
    args?.models?.map((m) => [m.name, m]) ?? [],
  );
  const serviceMap = Object.fromEntries(
    args?.services?.map((s) => [s.name, s]) ?? [],
  );

  // NOTE: these won't always be empty in real usage
  for (const model of Object.values(modelsMap)) {
    model.data_sources["default"] = {
      name: "default",
      is_private: false,
      tree: {},
      list_params: [],
    };
  }

  return {
    project_name: "test",
    models: modelsMap,
    services: serviceMap,
    poos: {},
    wrangler_env: {
      name: "Env",
      source_path: "source.ts",
      d1_bindings: ["d1"],
      kv_bindings: [],
      r2_bindings: [],
      vars: {},
    },
    main_source: null,
  };
}

abstract class ApiMethodBuilder {
  protected methods: Record<string, ApiMethod> = {};

  method(
    name: string,
    http_verb: HttpVerb,
    is_static: boolean,
    parameters: NamedTypedValue[],
    return_type: CidlType,
    return_media: MediaType = MediaType.Json,
    parameters_media: MediaType = MediaType.Json,
    data_source: string | null = null,
  ): this {
    this.methods[name] = {
      name,
      http_verb,
      is_static,
      parameters,
      return_type,
      return_media,
      parameters_media,
      data_source,
    };
    return this;
  }
}

export class IncludeTreeBuilder {
  private nodes: CidlIncludeTree = {};

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

  build(): CidlIncludeTree {
    return this.nodes;
  }
}

export class ModelBuilder {
  private name: string;
  private d1_binding: string | null = null;
  private primary_key_names: string[] = [];
  private primary_key_types: Record<string, CidlType> = {};
  private columns: D1Column[] = [];
  private navigation_properties: NavigationProperty[] = [];
  private key_params: string[] = [];
  private kv_objects: KeyValue[] = [];
  private r2_objects: AstR2Object[] = [];
  private methods: Record<string, ApiMethod> = {};
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
      value: { name, cidl_type },
      foreign_key_reference: foreign_key,
      unique_ids: [],
      composite_id: null,
    });
    return this;
  }

  navP(
    var_name: string,
    model_reference: string,
    kind: NavigationPropertyKind,
  ): this {
    this.navigation_properties.push({
      var_name,
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

  keyParam(name: string): this {
    this.key_params.push(name);
    return this;
  }

  kvObject(
    format: string,
    namespace_binding: string,
    name: string,
    list_prefix: boolean,
    cidl_type: CidlType,
  ): this {
    this.kv_objects.push({
      format,
      namespace_binding,
      value: { name, cidl_type },
      list_prefix,
    });
    return this;
  }

  r2Object(
    format: string,
    bucket_binding: string,
    var_name: string,
    list_prefix: boolean,
  ): this {
    this.r2_objects.push({
      format,
      bucket_binding,
      var_name,
      list_prefix,
    });
    return this;
  }

  method(
    name: string,
    http_verb: HttpVerb,
    is_static: boolean,
    parameters: NamedTypedValue[],
    return_type: CidlType,
    data_source: string | null = null,
  ): this {
    this.methods[name] = {
      name,
      http_verb,
      is_static,
      parameters,
      return_type,
      return_media: MediaType.Json,
      parameters_media: MediaType.Json,
      data_source,
    };
    return this;
  }

  dataSource(
    name: string,
    tree: any,
    is_private: boolean = false,
    list_params: CrudListParam[] = [],
  ): this {
    this.data_sources[name] = {
      name,
      tree,
      is_private,
      list_params,
    };
    return this;
  }

  crud(kind: CrudKind): this {
    this.cruds.push(kind);
    return this;
  }

  build(): Model {
    const mutableColumns = [...this.columns];
    const primary_key_columns: D1Column[] = [];

    for (const pkName of this.primary_key_names) {
      const idx = mutableColumns.findIndex((col) => col.value.name === pkName);
      if (idx >= 0) {
        primary_key_columns.push(mutableColumns[idx]);
        mutableColumns.splice(idx, 1);
      } else {
        primary_key_columns.push({
          value: {
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
      primary_key_columns,
      columns: mutableColumns,
      navigation_properties: this.navigation_properties,
      key_params: this.key_params,
      kv_objects: this.kv_objects,
      r2_objects: this.r2_objects,
      methods: this.methods,
      data_sources: this.data_sources,
      cruds: this.cruds,
      source_path: "",
    };
  }
}

export class ServiceBuilder extends ApiMethodBuilder {
  private name: string;
  private attributes: ServiceAttribute[] = [];

  constructor(name: string) {
    super();
    this.name = name;
  }

  static service(name: string) {
    return new ServiceBuilder(name);
  }

  inject(var_name: string, inject_reference: string): this {
    this.attributes.push({ var_name, inject_reference });
    return this;
  }

  build(): Service {
    return {
      name: this.name,
      attributes: this.attributes,
      methods: this.methods,
      source_path: "",
      initializer: null,
    };
  }
}
