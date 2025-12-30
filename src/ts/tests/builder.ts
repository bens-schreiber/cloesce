import {
  D1Model,
  CloesceAst,
  CidlIncludeTree,
  NamedTypedValue,
  HttpVerb,
  CidlType,
  NavigationPropertyKind,
  D1ModelAttribute,
  DataSource,
  D1NavigationProperty,
  ApiMethod,
  Service,
  ServiceAttribute,
  MediaType,
  KVModel,
  KVNavigationProperty,
  CrudKind,
} from "../src/ast";

export function createAst(args?: {
  d1Models?: D1Model[];
  kvModels?: KVModel[];
  services?: Service[];
}): CloesceAst {
  const d1ModelsMap = Object.fromEntries(
    args?.d1Models?.map((m) => [m.name, m]) ?? [],
  );
  const kvModelsMap = Object.fromEntries(
    args?.kvModels?.map((m) => [m.name, m]) ?? [],
  );
  const serviceMap = Object.fromEntries(
    args?.services?.map((s) => [s.name, s]) ?? [],
  );

  return {
    project_name: "test",
    d1_models: d1ModelsMap,
    kv_models: kvModelsMap,
    services: serviceMap,
    poos: {},
    wrangler_env: {
      name: "Env",
      source_path: "source.ts",
      d1_binding: "db",
      kv_bindings: [],
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

export class D1ModelBuilder extends ApiMethodBuilder {
  private name: string;
  private attributes: D1ModelAttribute[] = [];
  private navigation_properties: D1NavigationProperty[] = [];
  private primary_key: NamedTypedValue | null = null;
  private data_sources: Record<string, DataSource> = {};

  constructor(name: string) {
    super();
    this.name = name;
  }

  static model(name: string) {
    return new D1ModelBuilder(name);
  }

  attribute(
    name: string,
    cidl_type: CidlType,
    foreign_key: string | null = null,
  ): this {
    this.attributes.push({
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

  id(): this {
    return this.pk("id", "Integer");
  }

  dataSource(name: string, tree: CidlIncludeTree): this {
    this.data_sources[name] = {
      name,
      tree,
    };
    return this;
  }

  build(): D1Model {
    if (!this.primary_key) {
      throw new Error(`Model '${this.name}' has no primary key`);
    }

    return {
      name: this.name,
      attributes: this.attributes,
      navigation_properties: this.navigation_properties,
      primary_key: this.primary_key,
      methods: this.methods,
      data_sources: this.data_sources,
      cruds: [],
      source_path: "",
    };
  }
}

export class KVModelBuilder {
  private name: string;
  private binding: string;
  private cidl_type: CidlType;
  private params: string[] = [];
  private navigation_properties: KVNavigationProperty[] = [];
  private cruds: CrudKind[] = [];
  private methods: Record<string, ApiMethod> = {};
  private data_sources: Record<string, DataSource> = {};

  constructor(name: string, binding: string, cidl_type: CidlType) {
    this.name = name;
    this.binding = binding;
    this.cidl_type = cidl_type;
  }

  param(p: string): this {
    this.params.push(p);
    return this;
  }

  navP(name: string, cidl_type: CidlType): this {
    this.navigation_properties.push({
      KValue: { name, cidl_type },
    });
    return this;
  }

  modelNavP(model_reference: string, var_name: string, many: boolean): this {
    this.navigation_properties.push({
      Model: { model_reference, var_name, many },
    });
    return this;
  }

  method(name: string, api_method: ApiMethod): this {
    this.methods[name] = api_method;
    return this;
  }

  dataSource(name: string, tree: CidlIncludeTree): this {
    this.data_sources[name] = {
      name,
      tree,
    };
    return this;
  }

  build(): KVModel {
    return {
      name: this.name,
      binding: this.binding,
      cidl_type: this.cidl_type,
      params: [...this.params],
      navigation_properties: [...this.navigation_properties],
      cruds: [...this.cruds],
      methods: { ...this.methods },
      data_sources: { ...this.data_sources },
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
