Durable Objects are storage mechanisms able to host both SQLite and KV storage. They can be sharded by any number of instances.

A Durable Object can only be interfaced with from inside the Durable Object, exposing RPC endpoints capable of invoking methods on the Durable Object.

Currently, Cloesce states that all relationships are to be resolved within that Durable Object. This new Query Planner update will change that:
- Relationships can be resolved across Durable Objects

In order to make SQLite and KV storage available to outside sources, two generic methods will be added to every Cloesce Durable Object:
- `sql(query: string, args: any[]): any[]` - Executes a SQL query on the Durable Object's SQLite database and returns the results exactly as they would be returned from a SQLite query.
- `kv(key: string): any` - Retrieves a value from the Durable Object's KV storage.

When SQLite is used, the Query Planner will follow the same semantics as the D1 Query Planner.

# Runtime Context

When a `GET`, `LIST` or `SAVE` operation is executed on a Model, the ORM will generate a query plan that will be executed by the runtime. Two scenarios can occur when trying to execute the plan:
1. The code is being executed inside the Durable Object that contains the Model being queried
2. The code is executed outside of the Durable Object that contains the Model being queried (e.g. from a Worker, another Durable Object)

For now, the runtime will always call the stub methods `sql` and `kv`, but in the future, the runtime should be able to execute in the DO itself.

# Notes

- If a relationship from two models spans the same Durable Object backings (not necessarily the same shard), the Query Planner _may_ be able to fetch in a single query _iff_ the shard ID is passed from the base model to the related model. If the shard ID is not passed, the Query Planner will generate two separate queries to be executed on two different Durable Objects.
    - If a DO has no sharding at all (i.e. no `shard` block in the declaration), and a relationship from two models spans the same Durable Object backings, the Query Planner will _always_ be able to fetch in a single query.

- If a relationship from two models spans completely different Durable Object backings, two separate queries will _always_ be generated to be executed on two different Durable Objects (it is impossible to execute a single query across two different Durable Objects).

# Appendix

- [get](./get.md)
- [list](./list.md)