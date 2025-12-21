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
  NavigationProperty,
  ApiMethod,
  Service,
  ServiceAttribute,
  MediaType,
  KVModel,
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
    version: "1.0",
    project_name: "test",
    language: "TypeScript",
    d1_models: d1ModelsMap,
    kv_models: kvModelsMap,
    services: serviceMap,
    poos: {},
    wrangler_env: {
      name: "Env",
      source_path: "source.ts",
      db_binding: "db",
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
  private navigation_properties: NavigationProperty[] = [];
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
    model_name: string,
    kind: NavigationPropertyKind,
  ): this {
    this.navigation_properties.push({
      var_name,
      model_name,
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

export class KVModelBuilder extends ApiMethodBuilder {
  private name: string;
  private cidl_type: CidlType | null = null;
  private binding: string;

  constructor(name: string) {
    super();
    this.name = name;
  }

  static model(name: string) {
    return new KVModelBuilder(name);
  }

  value(cidl_type: CidlType): this {
    this.cidl_type = cidl_type;
    return this;
  }

  bind(binding: string): this {
    this.binding = binding;
    return this;
  }

  build(): KVModel {
    return {
      name: this.name,
      cidl_type: this.cidl_type!,
      binding: this.binding,
      source_path: "",
      methods: this.methods,
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

  inject(var_name: string, injected: string): this {
    this.attributes.push({ var_name, injected });
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
