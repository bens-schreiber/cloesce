import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
import { Either, left, right } from "../common.js";
import {
  RuntimeContainer,
  WasmResource,
  fromSql,
  invokeWasm,
} from "../runtime/runtime.js";

export { cloesce } from "../runtime/runtime.js";
export type { HttpResult, Either } from "../common.js";

// Compiler hints
export const D1: ClassDecorator = () => {};
export const PlainOldObject: ClassDecorator = () => {};
export const WranglerEnv: ClassDecorator = () => {};
export const PrimaryKey: PropertyDecorator = () => {};
export const GET: MethodDecorator = () => {};
export const POST: MethodDecorator = () => {};
export const PUT: MethodDecorator = () => {};
export const PATCH: MethodDecorator = () => {};
export const DELETE: MethodDecorator = () => {};
export const DataSource: PropertyDecorator = () => {};
export const OneToMany =
  (_: string): PropertyDecorator =>
  () => {};
export const OneToOne =
  (_: string): PropertyDecorator =>
  () => {};
export const ManyToMany =
  (_: string): PropertyDecorator =>
  () => {};
export const ForeignKey =
  <T>(_: T): PropertyDecorator =>
  () => {};
export const Inject: ParameterDecorator = () => {};

// Include Tree
type Primitive = string | number | boolean | bigint | symbol | null | undefined;
export type IncludeTree<T> = T extends Primitive
  ? never
  : {
      [K in keyof T]?: T[K] extends (infer U)[]
        ? IncludeTree<NonNullable<U>>
        : IncludeTree<NonNullable<T[K]>>;
    };

type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? K : never;
}[keyof T];

/**
 * ORM functions which use metadata to translate arguments to valid SQL queries.
 */
export class Orm {
  private constructor(private db: D1Database) {}

  /**
   * Creates an instance of an `Orm`
   * @param db The database to use for ORM calls.
   */
  static fromD1(db: D1Database): Orm {
    return new Orm(db);
  }

  /**
   * Maps SQL records to an instantiated Model. The records must be flat
   * (e.g., of the form "id, name, address") or derive from a Cloesce data source view
   * (e.g., of the form "Horse.id, Horse.name, Horse.address")
   * @param ctor The model constructor
   * @param records D1 Result records
   * @param includeTree Include tree to define the relationships to join.
   * @returns
   */
  static fromSql<T extends object>(
    ctor: new () => T,
    records: Record<string, any>[],
    includeTree: IncludeTree<T> | null
  ): Either<string, T[]> {
    return fromSql(ctor, records, includeTree);
  }

  /**
   * Returns a SQL query to insert a model into the database. Uses an IncludeTree as a guide for
   * foreign key relationships, only inserting the explicitly stated pattern in the tree.
   *
   * TODO: We should be able to leave primary keys and foreign keys undefined, with
   * primary keys being auto incremented and foreign keys being assumed by navigation property
   * context.
   *
   * @param ctor A model constructor.
   * @param newModel The new model to insert.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns Either an error string, or the insert query string.
   */
  static insertQuery<T extends object>(
    ctor: new () => T,
    newModel: T,
    includeTree: IncludeTree<T> | null
  ): Either<string, string> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(JSON.stringify(newModel), wasm),
      WasmResource.fromString(JSON.stringify(includeTree), wasm),
    ];
    return invokeWasm(wasm.insert_model, args, wasm);
  }

  /**
   * Returns a SQL query to update a model. Uses an IncludeTree as a guide for
   * foreign key relationships, only updating the explicitly stated pattern in the tree.
   *
   * @param ctor A model constructor.
   * @param updatedModel Updated values to insert. Non-updated values can be left undefined.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns Either an error string, or the insert query string.
   */
  static updateQuery<T extends object>(
    ctor: new () => T,
    updatedModel: Partial<T>,
    includeTree: IncludeTree<T> | null
  ): Either<string, string> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(JSON.stringify(updatedModel), wasm),
      WasmResource.fromString(JSON.stringify(includeTree), wasm),
    ];
    return invokeWasm(wasm.update_model, args, wasm);
  }

  /**
   * Executes an insert query on the database, using an IncludeTree as a guide for foreign key
   * relationships, only updating the explicitly stated pattern in the tree.
   *
   * @param ctor A model constructor.
   * @param newModel The new model to insert.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns An error string, or the primary key of the inserted model.
   */
  async insert<T extends object>(
    ctor: new () => T,
    newModel: T,
    includeTree: IncludeTree<T> | null
  ): Promise<Either<string, any>> {
    let insertQueryRes = Orm.insertQuery(ctor, newModel, includeTree);
    if (!insertQueryRes.ok) {
      return insertQueryRes;
    }

    // Split the query into individual statements.
    const statements = insertQueryRes.value
      .split(";")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);

    // One of these statements is a "SELECT", which is the root model id stmt.
    let selectIndex: number;
    for (let i = statements.length - 1; i >= 0; i--) {
      if (/^SELECT/i.test(statements[i])) {
        selectIndex = i;
        break;
      }
    }

    // Execute all statements in a batch.
    const batchRes = await this.db.batch(
      statements.map((s) => this.db.prepare(s))
    );

    if (!batchRes.every((r) => r.success)) {
      const failed = batchRes.find((r) => !r.success);
      return left(
        failed?.error ?? "D1 batch failed, but no error was returned."
      );
    }

    // Return the result of the SELECT statement
    const selectResult = batchRes[selectIndex!].results[0] as { id: any };

    return right(selectResult.id);
  }

  /**
   * Executes an update query on the database, using an IncludeTree as a guide for foreign key
   * relationships, only updating the explicitly stated pattern in the tree.
   *
   * @param ctor A model constructor.
   * @param updatedModel Updated values to insert. Non-updated values can be left undefined.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns An error string, or nothing.
   */
  async update<T extends object>(
    ctor: new () => T,
    updatedModel: Partial<T>,
    includeTree: IncludeTree<T> | null
  ): Promise<Either<string, undefined>> {
    let updateQueryRes = Orm.updateQuery(ctor, updatedModel, includeTree);
    if (!updateQueryRes.ok) {
      return updateQueryRes;
    }

    let d1Res = await this.db.prepare(updateQueryRes.value).run();
    if (!d1Res.success) {
      return left(d1Res.error ?? "D1 failed, but no error was returned.");
    }

    return right(undefined);
  }

  /**
   * Returns a query of the form `SELECT * FROM [Model.DataSource]`
   */
  static listQuery<T extends object>(
    ctor: new () => T,
    includeTree: KeysOfType<T, IncludeTree<T>> | null
  ): string {
    if (includeTree) {
      return `SELECT * FROM [${ctor.name}.${includeTree.toString()}]`;
    }

    return `SELECT * FROM [${ctor.name}]`;
  }

  /**
   * Returns a query of the form `SELECT * FROM [Model.DataSource] WHERE [Model.PrimaryKey] = ?`.
   * Requires the id parameter to be bound (use db.prepare().bind)
   */
  static getQuery<T extends object>(
    ctor: new () => T,
    includeTree: KeysOfType<T, IncludeTree<T>> | null
  ): string {
    const { ast } = RuntimeContainer.get();
    if (includeTree) {
      return `${this.listQuery(ctor, includeTree)} WHERE [${ctor.name}.${ast.models[ctor.name].primary_key.name}] = ?`;
    }

    return `${this.listQuery(ctor, includeTree)} WHERE [${ast.models[ctor.name].primary_key.name}] = ?`;
  }

  /**
   * Executes a query of the form `SELECT * FROM [Model.DataSource]`, returning all results
   * as instantiated models.
   */
  async list<T extends object>(
    ctor: new () => T,
    includeTreeKey: KeysOfType<T, IncludeTree<T>> | null
  ): Promise<Either<string, T[]>> {
    const q = Orm.listQuery(ctor, includeTreeKey);
    const res = await this.db.prepare(q).run();

    if (!res.success) {
      return left(res.error ?? "D1 failed but no error was returned.");
    }

    const { ast } = RuntimeContainer.get();
    const includeTree =
      includeTreeKey === null
        ? null
        : ast.models[ctor.name].data_sources[includeTreeKey.toString()].tree;

    const fromSqlRes = fromSql<T>(ctor, res.results, includeTree);
    if (!fromSqlRes.ok) {
      return fromSqlRes;
    }

    return right(fromSqlRes.value);
  }

  /**
   * Executes a query of the form `SELECT * FROM [Model.DataSource] WHERE [Model.PrimaryKey] = ?`
   * returning all results as instantiated models.
   */
  async get<T extends object>(
    ctor: new () => T,
    id: any,
    includeTreeKey: KeysOfType<T, IncludeTree<T>> | null
  ): Promise<Either<string, T>> {
    const q = Orm.getQuery(ctor, includeTreeKey);
    const res = await this.db.prepare(q).bind(id).run();

    if (!res.success) {
      return left(res.error ?? "D1 failed but no error was returned.");
    }

    const { ast } = RuntimeContainer.get();
    const includeTree =
      includeTreeKey === null
        ? null
        : ast.models[ctor.name].data_sources[includeTreeKey.toString()].tree;

    const fromSqlRes = fromSql<T>(ctor, res.results, includeTree);
    if (!fromSqlRes.ok) {
      return fromSqlRes;
    }

    return right(fromSqlRes.value[0]);
  }
}
