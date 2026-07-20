import { expect } from "vitest";
import { executeSave } from "../../src/router/executor/index.js";
import type {
  Database,
  MapCardinality,
  PathSegment,
  SaveArg,
  SavePlan,
  SaveStep,
  SqlStatement,
  TemplateSegment,
} from "../../src/router/executor/plan.js";
import { d1 } from "./executor.js";

export function write(sql: string, args: SaveArg[] = []): SqlStatement {
  return {
    Write: { sql, arguments: args },
  };
}
export function hydrate(sql: string, result: PathSegment[], args: SaveArg[] = []): SqlStatement {
  return {
    Hydrate: { sql, arguments: args, result },
  };
}
export function payload(v: unknown): SaveArg {
  return { Payload: v };
}
export function resultRef(path: PathSegment[]): SaveArg {
  return { Result: path };
}
export function field(name: string): PathSegment {
  return { Field: name };
}
export function index(i: number): PathSegment {
  return { Index: i };
}

export function savePlan(...stages: SaveStep[][]): SavePlan {
  return {
    stages: stages.map((steps) => ({ steps })),
  };
}

export function batchStep(
  result: PathSegment[],
  statements: SqlStatement[],
  opts: { db?: Database; shard?: [string, SaveArg][] } = {},
): SaveStep {
  return {
    result,
    query: {
      SqlBatch: {
        database: opts.db ?? d1(),
        shard: opts.shard ?? [],
        statements,
      },
    },
  };
}

export function keyWriteStep(
  result: PathSegment[],
  database: Database,
  segments: TemplateSegment<SaveArg>[],
  value: unknown,
  metadata: unknown | null = null,
  shard: [string, SaveArg][] = [],
): SaveStep {
  return {
    result,
    query: { KeyWrite: { database, segments, value, metadata, shard } },
  };
}

export function saveSynthStep(
  result: PathSegment[],
  fields: [string, SaveArg][],
  create: boolean,
  cardinality: MapCardinality = "One",
): SaveStep {
  return { result, query: { Synthesize: { fields, create, cardinality } } };
}

/** Run a save expected to sink no errors, returning the saved body. */
export async function executeSaveOk(...args: Parameters<typeof executeSave>): Promise<any> {
  const res = await executeSave(...args);
  expect(res.errors).toEqual([]);
  return res.value;
}
