import { expect } from "vitest";
import { executeSelect } from "../../src/router/executor/index.js";
import type {
  Database,
  MapCardinality,
  Mapping,
  Select,
  SelectArg,
  SelectPlan,
  SqlSegment,
  TableDef,
  TemplateSegment,
} from "../../src/router/executor/plan.js";
import { d1 } from "./executor.js";

export const one: Mapping = { cardinality: "One", join: [] };
export const many: Mapping = { cardinality: "Many", join: [] };
export function mapping(cardinality: MapCardinality, join: Mapping["join"] = []): Mapping {
  return {
    cardinality,
    join,
  };
}

export type RawArg = { Param: string } | { Field: string[] } | { ScalarField: string[] };
export function param(name: string): RawArg {
  return { Param: name };
}
export function spread(...path: string[]): RawArg {
  return { Field: path };
}
export function scalarField(...path: string[]): RawArg {
  return { ScalarField: path };
}

export type RawQuery =
  | {
      Sql: {
        database: Database;
        sql: string;
        arguments: RawArg[];
        mapping: Mapping;
        shard: [string, RawArg][];
        route_fields: [string, RawArg][];
      };
    }
  | { Key: { database: Database; segments: TemplateSegment<RawArg>[]; shard: [string, RawArg][] } }
  | { Synthesize: { fields: [string, RawArg][]; cardinality: MapCardinality } };
export type RawStep = { result: string[]; query: RawQuery };

/** Parse a `?N`-numbered SQL string into plan SqlSegments (Bind is 0-based). */
function sqlSegments(sql: string): SqlSegment[] {
  const segments: SqlSegment[] = [];
  const re = /\?(\d+)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(sql)) !== null) {
    if (m.index > last) {
      segments.push({ Literal: sql.slice(last, m.index) });
    }
    segments.push({ Bind: Number(m[1]) - 1 });
    last = m.index + m[0].length;
  }
  if (last < sql.length) {
    segments.push({ Literal: sql.slice(last) });
  }
  return segments;
}

/**
 * Each variadic argument is one stage's steps. Result string paths are assigned table ids
 * by first appearance (root `[]` = 0), and every arg's path is resolved to `{ table, field }`.
 */
export function selectPlan(...stages: RawStep[][]): SelectPlan {
  const ids = new Map<string, number>([[JSON.stringify([]), 0]]);
  const tables: TableDef[] = [{ parent: null }];
  for (const steps of stages) {
    for (const step of steps) {
      const key = JSON.stringify(step.result);
      if (!ids.has(key)) {
        const parentPath = step.result.slice(0, -1);
        ids.set(key, tables.length);
        tables.push({
          parent: {
            table: ids.get(JSON.stringify(parentPath))!,
            field: step.result[step.result.length - 1],
          },
        });
      }
    }
  }

  const tableOf = (path: string[]): number => {
    const id = ids.get(JSON.stringify(path));
    if (id === undefined) {
      throw new Error(`no table registered for path ${JSON.stringify(path)}`);
    }
    return id;
  };
  const resolveArg = (a: RawArg): SelectArg => {
    if ("Param" in a) {
      return { Param: a.Param };
    }
    const path = "Field" in a ? a.Field : a.ScalarField;
    return { Field: { table: tableOf(path.slice(0, -1)), field: path[path.length - 1] } };
  };
  const resolvePairs = (pairs: [string, RawArg][]): [string, SelectArg][] =>
    pairs.map(([f, a]) => [f, resolveArg(a)]);

  const resolveQuery = (q: RawQuery): Select => {
    if ("Sql" in q) {
      return {
        Sql: {
          database: q.Sql.database,
          sql: sqlSegments(q.Sql.sql),
          arguments: q.Sql.arguments.map((a) => ({ value: resolveArg(a), spread: "Field" in a })),
          mapping: q.Sql.mapping,
          shard: resolvePairs(q.Sql.shard),
          route_fields: resolvePairs(q.Sql.route_fields),
        },
      };
    }
    if ("Key" in q) {
      return {
        Key: {
          database: q.Key.database,
          segments: q.Key.segments.map((s) =>
            "Literal" in s ? s : { Value: resolveArg(s.Value) },
          ),
          shard: resolvePairs(q.Key.shard),
        },
      };
    }
    return {
      Synthesize: {
        fields: resolvePairs(q.Synthesize.fields),
        cardinality: q.Synthesize.cardinality,
      },
    };
  };

  return {
    tables,
    stages: stages.map((steps) => ({
      steps: steps.map((step) => ({
        table: tableOf(step.result),
        query: resolveQuery(step.query),
      })),
    })),
  };
}

export function sqlStep(
  result: string[],
  sql: string,
  opts: {
    args?: RawArg[];
    mapping?: Mapping;
    db?: Database;
    shard?: [string, RawArg][];
    route?: [string, RawArg][];
  } = {},
): RawStep {
  return {
    result,
    query: {
      Sql: {
        database: opts.db ?? d1(),
        sql,
        arguments: opts.args ?? [],
        mapping: opts.mapping ?? one,
        shard: opts.shard ?? [],
        route_fields: opts.route ?? [],
      },
    },
  };
}

export function keyStep(
  result: string[],
  database: Database,
  segments: TemplateSegment<RawArg>[],
  shard: [string, RawArg][] = [],
): RawStep {
  return { result, query: { Key: { database, segments, shard } } };
}

export function synthStep(
  result: string[],
  fields: [string, RawArg][],
  cardinality: MapCardinality = "One",
): RawStep {
  return { result, query: { Synthesize: { fields, cardinality } } };
}

/** Run a select expected to sink no errors, returning the hydrated body. */
export async function executeSelectOk(...args: Parameters<typeof executeSelect>): Promise<any> {
  const res = await executeSelect(...args);
  expect(res.errors).toEqual([]);
  return res.value;
}
