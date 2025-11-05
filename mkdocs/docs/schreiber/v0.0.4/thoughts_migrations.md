# Thoughts on Cloesce Migrations

Since Cloesce generates your SQL code via HLL-extracted models, it's important that Cloesce is also capable of generating the evolution of these models over time. For example, say we have some `Dog` model:

```ts
@D1
class Dog {
  @PrimaryKey
  id: number;
}
```

which has been committed to our database. Later, it's decided that the `Dog` model needs a name, so we modify:

```ts
@D1
class Dog {
  @PrimaryKey
  id: number;

  name: string;
}
```

How should this be reflected on D1? The simplest solution is to roll back the database, and then insert the `Dog` table as it is now. However, we can't just assume all developers want to drop their tables just to add a column. This is where SQL migrations come in. Instead of Cloesce generating an entirely new schema, it could have some kind of reference point from a past migration, and then use that migration to determine how to handle the next.

So, if the we knew the CIDL at this point, and it's been committed as migration:

```ts
@D1
class Dog {
  @PrimaryKey
  id: number;
}

// (in SQL) => INSERT TABLE "Dog" ...
```

Then the following migration's SQL would look something like

```ts
@D1
class Dog {
  @PrimaryKey
  id: number;

  name: string;
}

// (in SQL) => ALTER TABLE "Dog" ADD COLUMN "name" TEXT
```

## System Design

Currently, Cloesce undergoes a compilation process which generates SQL code along with all other output code with the `cloesce compile` command. All SQL code goes into `.generated/migrations/migrations.sql`. Being under the `.generated` directory, it's not expected for this code to be apart of the git commit history. So, in this newer version, what should we do with the generated SQL code? When should we generate it?

It will be important to seperate migrations from the compiler. Not every compilation of Cloesce inherently means a SQL table has changed (one could refactor/add/delete methods, POOs, etc). On top of this, changing a SQLite schema once committed is not trivial, as it requires careful manipulation of the tables and data inside of them.

Seperating these processes requires some refactors to the codebase. We will create another intermediate representation, a `cidl.pre.json`, a raw CIDL handed off from the extractor. The compilation process will focus only on validating this raw form, augmenting it so that it may be easily used for migrations (sorting models by insertion order, and undergoing the hash process needed for change detection (see next section)). Compilation will then produce a `cidl.json` on top of it's current artifacts.

Because of the constraints imposed by D1 and Wrangler (being, migrations must be pure SQL, no logic like in Entity Framework), it's easiest to add a **forward migrations only** engine. That means migrations can only be added, not removed.

We will support two main commands: `cloesce compile -- migrations add <name>` and `cloesce compile -- migrations update`.

Adding migrations will add a file in the format `migrations/<DATE>_<NAME>.sql` to a top level project directory. When ran, a full semantic analysis of the CIDL will occur, then producing a SQL file. A key difference is that D1 generation will no longer take just the CIDL as an input, but also the _last migrated_ CIDL, which will be stored under `migrations/<DATE>_<NAME>.json`. A diffing algorithm will occur, and only the changes will generate some kind of SQL code, be it adding a table/view, dropping a table/view, alerting a table/view, etc.

The `migrations update` command will take the most recent migration, and modify it with respect to the most recently generated CIDL, replacing the artifacts with a new date. This helps in the development process, where models might be modified frequently, but creating an entire new migration isn't really necessary because there may not be any real data in some hosted database, or the migration was created locally and hasn't been pushed upstream.

With this in mind, the process to get a Cloesce project up and running would look like:

```sh
cloesce compile -- migrations add Initial
wrangler d1 migrations apply <DB_NAME>

# or

cloesce compile -- migrations update
wrangler d1 reset <DB_NAME>             # might be necessary
wrangler d1 migrations apply <DB_NAME>
```

## AST Change Detection Algorithm

SQL generation will have to be shifted from additions only to track refactors and removals, given some working CIDL and some past CIDL (could be empty if not exists). But how do we actually diff two ASTs? We will try to do a structural hashing of the AST, creating a Merkle Tree.

First, let's determine what nodes of the AST are valuable for a hash:

- Model name
- Model primary key (name, cidl_type)
- Attributes (foreign key ref, name, cidl_type)
- Data source names
- Data source include trees
- Many to Many navigation properties

Thus, hashes will be needed for Models, Data Sources, Attributes and Navigation Properties. Where should we store these hashes? It's easiest to add a hash property to each part of the current CIDL Rust code, however, the full CIDL isn't really necessary for migrations. Thus, a secondary AST called a `MigrationsAst` will be created that is a subset of the `CloesceAst`, containing only the values we'd want to serialize for a migration. Rust's Serde helps out here, because we can read a `cidl.json` as a `MigrationsAst` despite it having extraneous fields.

The process for change detection will be as follows:

- During compilation, after semantic analysis succeeds, merkle hash the `CloesceAst` (parents hashes are composed of childrens hashes recursively)
- Save this hashed structure as `cidl.json`
- When a migration is ran, read the last migration as a `MigrationsAst` (which could be None), and read the current `cidl.json` as a `MigrationsAst`
- Then, traverse the trees and compare hashes. If there is no previous migration, everything is different, and we would then produce a full diff composed of only insert queries. If there are differences, the engine will determine how to handle them.
- Finally, save the output as a serialized `MigrationsAst`, which contains only the necessary values we need for migrations.

## The Renaming Problem

Let's say I rename `Dog` to `Cat`. From looking at the AST, do we know if `Dog` was renamed to `Cat`, or if `Dog` was dropped and `Cat` was added? It turns out we can't really know that. Entity Framework for instance has extensions in Visual Studio that will intercept class name refactors and associate the new name with a GUID associated with the last name. This isn't really something we can do. Another approach is Prisma, who defaults to dropping and adding tables or columns unless you explicitly say `@map("Dog")` on your `Cat` model. Another ORM, Drizzle provides an interactive prompt on migration which detects drops/renames and asks you to specify which one it is, and if it is a rename, specify what the previous column was. Entity Framework will also try to infer if something was a rename vs a drop with contextual clues, but it doesn't always work.

So, what is the best solution? I'm not a big fan of the inference approach, as we shouldn't try to infer things as important as this (we could drop tons of data if used incorrectly). I think we would be best off introducing a `@Rename` decorator like Prisma has, and then going the way of Drizzle with an interactive migrations prompt. Of course, migrations can always be hand written, or modified after generation. This is a best of both worlds I think. The `Rename` decorator will certainly have some consequences, and will need to be implemented very carefully.
