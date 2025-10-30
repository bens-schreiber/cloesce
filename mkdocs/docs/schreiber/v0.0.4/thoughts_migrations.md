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

Because of the constraints imposed by D1 and Wrangler (being, migrations must be pure SQL, no logic like in Entity Framework), the design we will go for in this migrations engine will be **forward migrations only**. That means migrations can only be added, not removed. In order to make this work as intended, SQL generation will be decoupled from compilation (though SQL semantic analysis will still occur during compilation).

We will support two main commands: `cloesce compile -- migrations add <name>` and `cloesce compile -- migrations update`.

Adding migrations will add a file in the format `migrations/<DATE>_<NAME>.sql` to a top level project directory. When ran, a full semantic analysis of the CIDL will occur, then producing a SQL file. A key difference is that D1 generation will no longer take just the CIDL as an input, but also the _last migrated_ CIDL, which will be stored under `migrations/<DATE>_<NAME>.json`. A diffing algorithm will occur, and only the changes will generate some kind of SQL code, be it adding a table/view, dropping a table/view, alerting a table/view, etc.

The `migrations update` command will take the most recent migration, and modify it with respect to the most recently generated CIDL, and the last migrated CIDL, replacing the most recent migration under the same name with a new date. This helps in the development process, where models might be modified frequently, but creating an entire new migration isn't really necessary because there may not be any real data in some hosted database, or the migration was created locally and hasn't been pushed upstream.

With this in mind, the process to get a Cloesce project up and running would look like:

```sh
cloesce compile -- migrations add Initial
wrangler d1 migrations apply <DB_NAME>

# or

cloesce compile -- migrations update
wrangler d1 reset <DB_NAME>
wrangler d1 migrations apply <DB_NAME>
```

## AST Change Detection Algorithm

SQL generation will have to be shifted to compute changes (additions, removals or refactors) between two CIDL's model AST's. But how do we actually diff two ASTs? We will try to do a structural hashing of the AST, similiar to a Merkle Tree.

First, let's determine what nodes of the AST are valuable for a hash:

- Model name
- Model primary key (name, cidl_type)
- Attributes (foreign key ref, name, cidl_type)
- Data source names
- Data source include trees
- Many to Many navigation properties

Thus, hashes will be stored on Models, Data Sources, Attributes and Navigation Properties. At compile time, we will hash the AST we are working on. Note that we will do semantic analysis on the AST by the time any migrations are being computed, so if someone renames a model `Dog` to `Cat` it's assured that there will be no invalid `Dog` references laying around. After semantic analysis, we can then traverse the models to find diffs between the working tree, and the last stored migration's tree. If a top level diff is equivalent, then no changes have been made. If a value is missing, then we know it has been either renamed or dropped-- we'll always assume dropped since we don't have the metadata to know if it has been renamed (this is a problem, see below section). If a value does not exist in the last AST, it's been added. Finally, if a value has a different hash, we will compare the values of those nodes, and recursively repeat the process for any child nodes.

All diffs will be moved into their own intermediate structure grouped by the model. Then, we will topologically traverse these changes. This is kind of tricky, because dropped tables will have to occur last, but the order of them has to be with respect to the previous AST. For example, if in AST 1:

```
Boss has a Person who has a Dog (name, age)
```

And in AST 2:

```
Dog (name)
```

We would need to modify Dog, then drop Boss, then drop Person. Of course, AST 2 does _not even know about Boss or Person_, so we have to sort the first AST topologically (backwards from insertion order which would be `Person -> Boss`), then sort the second, and merge the sorted orders. This should be as simple as traversing the first sorted AST, grabbing what is necessary, and then appending that list to the end of our second list.

For diffing Include Trees, I think for now we'll spare ourselves the work and just completely regenerate the tree each time.

## The Renaming Problem

Let's say I rename `Dog` to `Cat`. From looking at the AST, do we know if `Dog` was renamed to `Cat`, or if `Dog` was dropped and `Cat` was added? It turns out we can't really know that. Entity Framework for instance has extensions in Visual Studio that will intercept class name refactors and associate the new name with a GUID associated with the last name. This isn't really something we can do. Another approach is Prisma, who defaults to dropping and adding tables or columns unless you explicitly say `@map("Dog")` on your `Cat` model. Another ORM, Drizzle provides an interactive prompt on migration which detects drops/renames and asks you to specify which one it is, and if it is a rename, specify what the previous column was. Entity Framework will also try to infer if something was a rename vs a drop with contextual clues, but it doesn't always work.

So, what is the best solution? I'm not a big fan of the inference approach, as we shouldn't try to infer things as important as this (we could drop tons of data if used incorrectly). I think we would be best off introducing a `@Rename` decorator like Prisma has, and then going the way of Drizzle with an interactive migrations prompt. Of course, migrations can always be hand written, or modified after generation. This is a best of both worlds I think. The `Rename` decorator will certainly have some consequences, and will need to be implemented very carefully.
