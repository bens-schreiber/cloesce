/**
 * @internal
 * TypeScript mirror of the Cloesce query plan IR (see `src/compiler/orm/src/query`).
 *
 * These types are the JSON shapes produced by the WASM `plan_select` / `plan_save`
 * entry points and consumed by the runtime {@link executor}.
 */

/** The backing store a plan {@link Database} handle points at. */
export type DatabaseKind = "D1" | "DurableObject" | "Kv" | "R2";

export interface Database {
  name: string;
  kind: DatabaseKind;
}

/**
 * A segment of a KV/R2 key template. A `Literal` is copied verbatim; a `Value`
 * carries an argument to interpolate.
 */
export type TemplateSegment<A> = { Literal: string } | { Value: A };

export type MapCardinality = "One" | "Many";

export interface SelectPlan {
  stages: SelectStage[];
}

export interface SelectStage {
  steps: SelectStep[];
}

export interface SelectStep {
  query: Select;
  /** Path of field names to the slot this step attaches to; empty = root. */
  result: string[];
}

export type SelectArg = { Param: string } | { ParentField: string };

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
        sql: string;
        arguments: SelectArg[];
        mapping: Mapping;
        shard: [string, SelectArg][];
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
