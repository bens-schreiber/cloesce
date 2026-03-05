import {
  CloesceAst,
  NavigationProperty,
  NavigationPropertyKind,
} from "./ast.js";

export interface CloesceConfigOptions {
  /**
   * Source paths containing .cloesce.ts files
   */
  srcPaths: string[];

  /**
   * Project name (optional, defaults to package.json name)
   */
  projectName?: string;

  /**
   * Output directory for generated files (default: .generated)
   */
  outPath?: string;

  /**
   * Workers URL for API endpoints
   */
  workersUrl?: string;

  /**
   * Path to migrations directory (default: ./migrations)
   */
  migrationsPath?: string;

  /**
   * Whether to truncate source paths to just the filename
   */
  truncateSourcePaths?: boolean;

  astModifiers?: Array<(ast: CloesceAst) => void>;
}

export interface CloesceConfig extends CloesceConfigOptions {
  /**
   * Configure a Models D1 properties with a fluent API
   */
  model<T extends object>(
    model: new () => T,
    callback: (builder: ModelBuilder<T>) => void,
  ): CloesceConfig;

  /**
   * Modify the raw AST before generation
   */
  rawAst(callback: (ast: CloesceAst) => void): CloesceConfig;

  /**
   * @internal
   * Get all AST modifiers
   */
  _getAstModifiers(): Array<(ast: CloesceAst) => void>;
}

/**
 * Define a Cloesce configuration
 */
export function defineConfig(config: CloesceConfigOptions): CloesceConfig {
  return new CloesceConfigBuilder(config);
}

/** @internal */
export type DefaultCloesceConfig = Required<CloesceConfigOptions>;

/** @internal */
export function setDefaultConfigs(
  config: CloesceConfigOptions,
): DefaultCloesceConfig {
  return {
    srcPaths: config.srcPaths,
    projectName: config.projectName ?? "cloesce-project",
    outPath: config.outPath ?? ".generated",
    workersUrl: config.workersUrl ?? "http://localhost:8787",
    migrationsPath: config.migrationsPath ?? "./migrations",
    truncateSourcePaths: config.truncateSourcePaths ?? false,
    astModifiers: config.astModifiers ?? [],
  };
}

interface ForeignKeyDefinition {
  column: string;
  referencedModel: string;
  referencedColumn: string;
}

interface RelationshipDefinition {
  propertyName: string;
  kind: "OneToOne" | "OneToMany" | "ManyToMany";
  referencedModel: string;
  referencedColumn: string;
}

interface PrimaryKeyDefinition {
  column: string;
}

class ModelConfig {
  primaryKeys: PrimaryKeyDefinition[] = [];
  foreignKeys: ForeignKeyDefinition[] = [];
  relationships: RelationshipDefinition[] = [];
}

export class ForeignKeyBuilder<T extends object> {
  constructor(
    private column: string,
    private modelConfig: ModelConfig,
  ) {}

  references<R extends object>(
    model: new () => R,
    referenceColumn: keyof R,
  ): ModelBuilder<T> {
    const modelName = model.name;
    this.modelConfig.foreignKeys.push({
      column: this.column,
      referencedModel: modelName,
      referencedColumn: String(referenceColumn),
    });
    return new ModelBuilder<T>(this.modelConfig);
  }
}

export class RelationshipBuilder<T extends object> {
  constructor(
    private propertyName: string,
    private kind: "OneToOne" | "OneToMany" | "ManyToMany",
    private modelConfig: ModelConfig,
  ) {}

  references<R extends object>(
    model: new () => R,
    referenceColumn: keyof R,
  ): ModelBuilder<T> {
    const modelName = model.name;
    this.modelConfig.relationships.push({
      propertyName: this.propertyName,
      kind: this.kind,
      referencedModel: modelName,
      referencedColumn: String(referenceColumn),
    });
    return new ModelBuilder<T>(this.modelConfig);
  }
}

export class ModelBuilder<T extends object = any> {
  constructor(private modelConfig: ModelConfig = new ModelConfig()) {}

  primaryKey<K extends keyof T>(column: K): ModelBuilder<T> {
    this.modelConfig.primaryKeys.push({ column: String(column) });
    return this;
  }

  foreignKey<K extends keyof T>(column: K): ForeignKeyBuilder<T> {
    return new ForeignKeyBuilder<T>(String(column), this.modelConfig);
  }

  oneToOne<K extends keyof T>(propertyName: K): RelationshipBuilder<T> {
    return new RelationshipBuilder<T>(
      String(propertyName),
      "OneToOne",
      this.modelConfig,
    );
  }

  oneToMany<K extends keyof T>(propertyName: K): RelationshipBuilder<T> {
    return new RelationshipBuilder<T>(
      String(propertyName),
      "OneToMany",
      this.modelConfig,
    );
  }

  manyToMany<K extends keyof T>(propertyName: K): RelationshipBuilder<T> {
    return new RelationshipBuilder<T>(
      String(propertyName),
      "ManyToMany",
      this.modelConfig,
    );
  }
}

/** @internal */
export class CloesceConfigBuilder implements CloesceConfig {
  public srcPaths: string[];
  public projectName?: string;
  public outPath?: string;
  public workersUrl?: string;
  public migrationsPath?: string;
  public truncateSourcePaths?: boolean;
  public astModifiers: Array<(ast: CloesceAst) => void>;

  constructor(config: CloesceConfigOptions) {
    this.srcPaths = config.srcPaths;
    this.projectName = config.projectName;
    this.outPath = config.outPath;
    this.workersUrl = config.workersUrl;
    this.migrationsPath = config.migrationsPath;
    this.truncateSourcePaths = config.truncateSourcePaths;
    this.astModifiers = config.astModifiers ?? [];
  }

  /**
   * Configure a model with the fluent API
   */
  model<T extends object>(
    model: new () => T,
    callback: (builder: ModelBuilder<T>) => void,
  ): CloesceConfig {
    const modelName = model.name;
    const modelConfig = new ModelConfig();
    const builder = new ModelBuilder<T>(modelConfig);
    callback(builder);

    // Create an AST modifier that applies this model's configuration
    this.astModifiers.push((ast: CloesceAst) => {
      const astModel = ast.models[modelName];
      if (!astModel) {
        console.warn(`Model ${modelName} not found in AST`);
        return;
      }

      // Apply primary keys
      for (const pk of modelConfig.primaryKeys) {
        const column = astModel.columns.find((c) => c.value.name === pk.column);
        if (column) {
          astModel.primary_key = column.value;
        }
      }

      // Apply foreign keys
      for (const fk of modelConfig.foreignKeys) {
        const column = astModel.columns.find((c) => c.value.name === fk.column);
        if (column) {
          column.foreign_key_reference = fk.referencedModel;
        }
      }

      // Apply relationships (navigation properties)
      for (const rel of modelConfig.relationships) {
        let kind: NavigationPropertyKind;
        if (rel.kind === "OneToOne") {
          kind = { OneToOne: { column_reference: rel.referencedColumn } };
        } else if (rel.kind === "OneToMany") {
          kind = { OneToMany: { column_reference: rel.referencedColumn } };
        } else {
          kind = "ManyToMany";
        }

        const navProp: NavigationProperty = {
          var_name: rel.propertyName,
          model_reference: rel.referencedModel,
          kind,
        };

        const existingIndex = astModel.navigation_properties.findIndex(
          (np) => np.var_name === rel.propertyName,
        );

        if (existingIndex >= 0) {
          astModel.navigation_properties[existingIndex] = navProp;
        } else {
          astModel.navigation_properties.push(navProp);
        }
      }
    });

    return this;
  }

  /**
   * Modify the raw AST before generation
   */
  rawAst(callback: (ast: CloesceAst) => void): CloesceConfig {
    this.astModifiers.push(callback);
    return this;
  }

  /**
   * @internal
   * Get all AST modifiers
   */
  _getAstModifiers(): Array<(ast: CloesceAst) => void> {
    return this.astModifiers;
  }
}
