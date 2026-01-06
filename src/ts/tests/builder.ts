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
} from "../src/ast";

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

  return {
    project_name: "test",
    models: modelsMap,
    services: serviceMap,
    poos: {},
    wrangler_env: {
      name: "Env",
      source_path: "source.ts",
      d1_binding: "db",
      kv_bindings: [],
      r2_bindings: [],
      vars: {},
    },
    app_source: null,
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
  ): this {
    this.methods[name] = {
      name,
      http_verb,
      is_static,
      parameters,
      return_type,
      return_media,
      parameters_media,
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
  private primary_key: NamedTypedValue | null = null;
  private columns: D1Column[] = [];
  private navigation_properties: NavigationProperty[] = [];
  private key_params: string[] = [];
  private kv_objects: KeyValue[] = [];
  private r2_objects: AstR2Object[] = [];
  private methods: Record<string, ApiMethod> = {};
  private data_sources: Record<string, DataSource> = {};

  constructor(name: string) {
    this.name = name;
  }

  static model(name: string): ModelBuilder {
    return new ModelBuilder(name);
  }

  col(
    name: string,
    cidl_type: CidlType,
    foreign_key: string | null = null,
  ): this {
    this.columns.push({
      value: { name, cidl_type },
      foreign_key_reference: foreign_key,
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
    this.primary_key = { name, cidl_type };
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
    cidl_type: CidlType,
  ): this {
    this.kv_objects.push({
      format,
      namespace_binding,
      value: { name, cidl_type },
    });
    return this;
  }

  r2Object(
    format: string,
    bucket_binding: string,
    var_name: string,
  ): this {
    this.r2_objects.push({
      format,
      bucket_binding,
      var_name,
    });
    return this;
  }

  method(
    name: string,
    http_verb: HttpVerb,
    is_static: boolean,
    parameters: NamedTypedValue[],
    return_type: CidlType,
  ): this {
    this.methods[name] = {
      name,
      http_verb,
      is_static,
      parameters,
      return_type,
      return_media: MediaType.Json,
      parameters_media: MediaType.Json,
    };
    return this;
  }

  dataSource(name: string, tree: any): this {
    this.data_sources[name] = {
      name,
      tree,
    };
    return this;
  }

  build(): Model {
    return {
      name: this.name,
      primary_key: this.primary_key,
      columns: this.columns,
      navigation_properties: this.navigation_properties,
      key_params: this.key_params,
      kv_objects: this.kv_objects,
      r2_objects: this.r2_objects,
      methods: this.methods,
      data_sources: this.data_sources,
      cruds: [],
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
    };
  }
}
