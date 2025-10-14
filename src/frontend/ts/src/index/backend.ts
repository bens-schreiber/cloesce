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
    includeTree: IncludeTree<T> | null,
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
    includeTree: IncludeTree<T> | null,
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
    includeTree: IncludeTree<T> | null,
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
   * @returns An error string, or nothing.
   */
  async insert<T extends object>(
    ctor: new () => T,
    newModel: T,
    includeTree: IncludeTree<T> | null,
  ): Promise<Either<string, undefined>> {
    let insertQueryRes = Orm.insertQuery(ctor, newModel, includeTree);
    if (!insertQueryRes.ok) {
      return insertQueryRes;
    }

    let d1Res = await this.db.prepare(insertQueryRes.value).run();
    if (!d1Res.success) {
      return left(d1Res.error ?? "D1 failed, but no error was returned.");
    }

    return right(undefined);
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
    includeTree: IncludeTree<T> | null,
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
}
