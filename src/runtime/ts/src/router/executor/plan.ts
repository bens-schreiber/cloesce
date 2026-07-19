/**
 * @internal
 * TypeScript mirror of the Cloesce query plan IR (see `src/compiler/orm/src/query`).
 *
 * A plan is a sequence of stages:
 * - Every step within a stage is independent
 * - Each stage runs all of its steps concurrently.
 * - The results produced by each step are sunk sequentially in step order.
 * - A failed step does not halt the plan, but no-ops any dependent steps in later stages.
 * - All failures are collected and surfaced in the final result.
 */

//#region: Shared IR
/** The backing store a plan {@link Database} handle points at. */
export type DatabaseKind = "D1" | "DurableObject" | "Kv" | "R2";

export interface Database {
  name: string;
  kind: DatabaseKind;
}

/**
 * A segment of a KV/R2 key template.
 *
 * - `Literal` is copied verbatim
 * - `Value` carries an argument to interpolate.
 */
export type TemplateSegment<A> = { Literal: string } | { Value: A };

export type MapCardinality = "One" | "Many";
//#endregion: Shared IR

//#region: Select IR
export interface SelectPlan {
  tables: TableDef[];
  stages: SelectStage[];
}

export interface TableDef {
  parent: { table: number; field: string } | null;
}

export interface SelectStage {
  steps: SelectStep[];
}

export interface SelectStep {
  query: Select;
  table: number;
}

export type SelectArg = { Param: string } | { Field: { table: number; field: string } };

export type SqlSegment = { Literal: string } | { Bind: number };

export interface SqlArgument {
  value: SelectArg;
  spread: boolean;
}

export interface JoinKeys {
  parent_key: string;
  child_key: string;
}

export interface Mapping {
  cardinality: MapCardinality;
  join: JoinKeys[];
}

export type Select =
  | {
      Sql: {
        database: Database;
        sql: SqlSegment[];
        arguments: SqlArgument[];
        mapping: Mapping;
        shard: [string, SelectArg][];
        route_fields: [string, SelectArg][];
      };
    }
  | {
      Key: {
        database: Database;
        segments: TemplateSegment<SelectArg>[];
        shard: [string, SelectArg][];
      };
    }
  | {
      Synthesize: {
        fields: [string, SelectArg][];
        cardinality: MapCardinality;
      };
    };
//#endregion: SelectIR

//#region: Save IR
export type PathSegment = { Field: string } | { Index: number };

export interface SavePlan {
  stages: SaveStage[];
}

export interface SaveStage {
  steps: SaveStep[];
}

export interface SaveStep {
  query: SaveQuery;
  result: PathSegment[];
}

/** An argument to a save statement: a literal payload value or a hydrated-body reference. */
export type SaveArg = { Payload: unknown } | { Result: PathSegment[] };

export type SqlStatement =
  | {
      Write: {
        sql: string;
        arguments: SaveArg[];
      };
    }
  | {
      Hydrate: {
        sql: string;
        arguments: SaveArg[];
        result: PathSegment[];
      };
    };

export type SaveQuery =
  | {
      SqlBatch: {
        database: Database;
        statements: SqlStatement[];
        shard: [string, SaveArg][];
      };
    }
  | {
      KeyWrite: {
        database: Database;
        segments: TemplateSegment<SaveArg>[];
        value: unknown;
        metadata: unknown | null;
        shard: [string, SaveArg][];
      };
    }
  | {
      Synthesize: {
        fields: [string, SaveArg][];
        create: boolean;
        cardinality: MapCardinality;
      };
    };
//#endregion: Save IR
