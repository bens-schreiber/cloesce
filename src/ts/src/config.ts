import {
  CloesceAst,
  NavigationProperty,
  NavigationPropertyKind,
} from "./ast.js";

let nextCompositeId = 0;
function compositeIdGen(): number {
  const id = nextCompositeId;
  nextCompositeId += 1;
  return id;
}

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
  columns: string[];
  referencedModel: string;
  referencedColumns: string[];
  compositeId: number | null;
}

type RelationshipDefinition =
  | {
      propertyName: string;
      kind: "OneToOne";
      referencedModel: string;
      keyColumns: string[];
    }
  | {
      propertyName: string;
      kind: "OneToMany" | "ManyToMany";
      referencedModel: string;
      referencedColumns: string[];
    };

interface PrimaryKeyDefinition {
  columns: string[];
}

interface UniqueConstraintDefinition {
  id: number;
  columns: string[];
}

class ModelConfig {
  primaryKeys: PrimaryKeyDefinition[] = [];
  foreignKeys: ForeignKeyDefinition[] = [];
  relationships: RelationshipDefinition[] = [];
  uniqueConstraints: UniqueConstraintDefinition[] = [];
}

export class ForeignKeyBuilder<T extends object> {
  constructor(
    private columns: string[],
    private modelConfig: ModelConfig,
  ) {}

  references<R extends object>(
    model: new () => R,
    ...referenceColumns: (keyof R)[]
  ): ModelBuilder<T> {
    if (this.columns.length !== referenceColumns.length) {
      throw new Error(
        `Foreign key definition mismatch: ${this.columns.length} local column(s) but ${referenceColumns.length} referenced column(s).`,
      );
    }

    const localColumns = [...this.columns];
    const referencedColumns = referenceColumns.map((col) => String(col));

    const modelName = model.name;
    this.modelConfig.foreignKeys.push({
      columns: localColumns,
      referencedModel: modelName,
      referencedColumns,
      compositeId: localColumns.length > 1 ? compositeIdGen() : null,
    });
    return new ModelBuilder<T>(this.modelConfig);
  }
}

export class RelationshipBuilder<T extends object> {
  constructor(
    private propertyName: string,
    private kind: "OneToMany" | "ManyToMany",
    private modelConfig: ModelConfig,
  ) {}

  references<R extends object>(
    model: new () => R,
    ...referenceColumns: (keyof R)[]
  ): ModelBuilder<T> {
    const modelName = model.name;
    this.modelConfig.relationships.push({
      propertyName: this.propertyName,
      kind: this.kind,
      referencedModel: modelName,
      referencedColumns: referenceColumns.map((col) => String(col)),
    });
    return new ModelBuilder<T>(this.modelConfig);
  }
}

export class OneToOneRelationshipBuilder<T extends object> {
  constructor(
    private propertyName: string,
    private modelConfig: ModelConfig,
  ) {}

  references<R extends object>(
    model: new () => R,
    ...keyColumns: (keyof T)[]
  ): ModelBuilder<T> {
    const modelName = model.name;
    this.modelConfig.relationships.push({
      propertyName: this.propertyName,
      kind: "OneToOne",
      referencedModel: modelName,
      keyColumns: keyColumns.map((col) => String(col)),
    });
    return new ModelBuilder<T>(this.modelConfig);
  }
}

export class ModelBuilder<T extends object = any> {
  constructor(private modelConfig: ModelConfig = new ModelConfig()) {}

  primaryKey<K extends keyof T>(...columns: K[]): ModelBuilder<T> {
    if (columns.length === 0) {
      throw new Error("primaryKey requires at least one column");
    }

    this.modelConfig.primaryKeys.push({
      columns: columns.map((col) => String(col)),
    });
    return this;
  }

  foreignKey<K extends keyof T>(...columns: K[]): ForeignKeyBuilder<T> {
    if (columns.length === 0) {
      throw new Error("foreignKey requires at least one column");
    }

    return new ForeignKeyBuilder<T>(
      columns.map((col) => String(col)),
      this.modelConfig,
    );
  }

  oneToOne<K extends keyof T>(propertyName: K): OneToOneRelationshipBuilder<T> {
    return new OneToOneRelationshipBuilder<T>(
      String(propertyName),
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

  unique<K extends keyof T>(...columns: K[]): ModelBuilder<T> {
    if (columns.length === 0) {
      return this;
    }

    const columnNames = columns.map((c) => String(c));
    const id = this.modelConfig.uniqueConstraints.length;
    this.modelConfig.uniqueConstraints.push({
      id,
      columns: columnNames,
    });
    return this;
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

      const allColumns = [...astModel.columns, ...astModel.primary_key_columns];
      const columnsByName = new Map(
        allColumns.map((column) => [column.value.name, column] as const),
      );
      const warnMissingColumn = (columnName: string) => {
        console.warn(`Column ${columnName} not found in model ${modelName}`);
      };

      // Apply primary keys
      if (modelConfig.primaryKeys.length > 0) {
        const nextPrimaryKeyColumns = [];
        const primaryKeyNames = new Set<string>();

        for (const pk of modelConfig.primaryKeys) {
          for (const pkColumnName of pk.columns) {
            const column = columnsByName.get(pkColumnName);
            if (!column) {
              warnMissingColumn(pkColumnName);
              continue;
            }

            if (primaryKeyNames.has(pkColumnName)) {
              continue;
            }

            primaryKeyNames.add(pkColumnName);
            column.composite_id = null;
            nextPrimaryKeyColumns.push(column);
          }
        }

        astModel.primary_key_columns = nextPrimaryKeyColumns;
        astModel.columns = allColumns.filter(
          (column) => !primaryKeyNames.has(column.value.name),
        );
      }

      // Apply foreign keys
      for (const fk of modelConfig.foreignKeys) {
        for (let i = 0; i < fk.columns.length; i += 1) {
          const columnName = fk.columns[i];
          const referencedColumnName = fk.referencedColumns[i];

          const column = columnsByName.get(columnName);
          if (!column) {
            warnMissingColumn(columnName);
            continue;
          }

          column.foreign_key_reference = {
            model_name: fk.referencedModel,
            column_name: referencedColumnName,
          };

          column.composite_id = fk.compositeId;
        }
      }

      // Apply unique constraints
      for (const unique of modelConfig.uniqueConstraints) {
        for (const columnName of unique.columns) {
          const column = columnsByName.get(columnName);
          if (!column) {
            warnMissingColumn(columnName);
            continue;
          }

          column.unique_ids.push(unique.id);
        }
      }

      // Apply navigation properties
      for (const rel of modelConfig.relationships) {
        let kind: NavigationPropertyKind;
        if (rel.kind === "OneToOne") {
          kind = { OneToOne: { key_columns: rel.keyColumns } };
        } else if (rel.kind === "OneToMany") {
          kind = { OneToMany: { key_columns: rel.referencedColumns } };
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
