# Thoughts on v0.0.1 SQL Generation

General ideas on working through [milestone 1](https://github.com/bens-schreiber/cloesce/milestone/1)

## Milestone 1

The SQL generator takes the Cloesce IDL as input and is capable of outputting the correct SQL interpretation of the model, as well as the Wrangler file setup. For v0.0.1 our goals are:

- A rust process that can take the CIDL as input, elegantly error handle format deviations
- Interpet a json model as a SQL table in the default SQL database, with any Sqlite type, and with primary keys
- Output the correct D1 infrastructure config as a Wrangler file

By the end of this version, we should be capable of creating a Cloudflare deployable D1 database from CIDL models.

## Approach

This initial version should be very simple, all we need to do is:

1. Convert CIDL model => SQLite
2. Add the correct database to the wrangler file
3. Run migrations

Really, the only part that has nuance is generating the SQLite schema, however, we have tons of options within Rust to do that. As of right now, we are deciding to keep the CIDL simple and just have attributes be JSON. An example input would be:

```json
{
  "Person": {
    "columns": {
      "id": { "primary_key": true, "type": 0 },
      "name": { "type": 1, "nullable": false }
    },
    .
    .
    .
  }
}
```

After generation, we would make the following SQL (for `default.db`):

```sql
CREATE TABLE Person (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
);
```

Note SQLite only has [5 types](https://www.sqlite.org/datatype3.html) (including NULL).

Choosing the right tool for this will be important. There are hundreds of SQL-Query builders, so we don't need to reinvent the wheel there. [Sea Query](https://github.com/SeaQL/sea-query) seems promising in it's easy fluent table creation.

Lastly we need to create the wrangler file. It will be important to not _replace_ the existing wrangler file, but only the relevant fields.

```toml
[[d1_databases]]
binding = "some_binding_name"
database_name = "default"
```

## Tests

There are a couple domains that need to be tested here:

- CIDL => In Memory Deserialization
- Deserialization => Sqlite
- Deserialization => Wrangler

For deserializing the CIDL, a series of simple unit tests covering all edge cases should be fine. For translating into Sqlite, we can mostly assume that Sea Query has us covered on not producing error prone Sqlite, so we only really need to test that the sql output has the correct fields, through unit tests and snapshot tests. Finally, the Wrangler output can be easily unit tested, and snapshot tested.

For sanity, a full integration snapshot test accepting a valid CIDL and outputting a file, as well as launching a Sqlite DB should cover all bases.

Wrangler provides a couple checks, mainly:

```bash
wrangler check   # verify config

wrangler build

wrangler dev    # run the dev environment

wrangler publish --dry-run  # simulate deployment
```

For a full integration test, these commands should be ran as well.

## Foreign Keys

For this milestone, foreign keys aren't going to be supported, however, our MVP `v0.1.0` will support any kind of relationship, so we should consider it in this initial design as well.

Ideally, Cloesce can utilize the same design that .NET's Entity Framework achieves. For example, declaring a 1:M relationship in Entity Framework looks like:

```C#
public class Blog
{
    public int BlogId { get; set; }
    public string Url { get; set; }

    public List<Post> Posts { get; } = new();
}

public class Post
{
    public int PostId { get; set; }
    public string Title { get; set; }
    public string Content { get; set; }

    public int BlogId { get; set; }
    public Blog Blog { get; set; }
}
```

which would generate the Sqlite

```sql
CREATE TABLE Blogs (
    BlogId INTEGER PRIMARY KEY,
    Url TEXT
);

CREATE TABLE Posts (
    PostId INTEGER PRIMARY KEY,
    Title TEXT,
    Content TEXT,
    BlogId INTEGER NOT NULL,
    FOREIGN KEY (BlogId) REFERENCES Blogs(BlogId) ON DELETE CASCADE
);

CREATE INDEX IX_Posts_BlogId ON Posts(BlogId);
```

There's a lot to take in. First, looking a the `Blog` model, there is a defined `Posts` field, but we can see in the Sqlite output `Blog` has no array of `Posts`. This is because Sqlite (really, most database languages) have no concept of an array, only foreign keys. `Posts` is a "navigation property", meaning if I had a `Blog` model I would need to explicitly fetch it for it to populate:

```C#
var blog = db.Blogs.Include(b => b.Posts).First();
```

In order to copy this pattern in Cloesce, a similiar function would have to exist. Note that if `Include` is not called, `Posts` will be empty, or in the case of a 1:1 relationship, it would be null.

```C#
public class Person
{
    public int PersonId { get; set; }
    public Passport Passport { get; set; }  // navigation property, null if not included
}

public class Passport
{
    public int PassportId { get; set; }
    public int PersonId { get; set; } // always defined
    public Person Person { get; set; } // navigation property, null if not included
}
```

Since Cloesce aims to function entirely from the IDL, creating an ORM to sit client side would make us slower to adapt to new languages (instead of just writing a generator, we now need to support ORM libraries for each language).

Because of this, I think we should avoid navigation properties for the time being, though, I'd be open to lightweight solutions.
