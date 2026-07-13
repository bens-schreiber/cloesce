# Implement Save/Upsert

Now that GET/LIST is working, we can implement the SAVE operation (sometimes called upsert, but from now on, always call it SAVE).

This will be a new query type that is very different to GET/LIST.

The current upsert algorithm creates a series of INSERT/UPDATE statements for each model in SQL, and then executes them in order in one single transaction. It uses an intermediary `$cloesce_tmp` table to store new primary keys such that they can be used in subsequent INSERTs.

The next algorithm will be similar, but will be able to do many different kinds of databases and storages.

The current upsert algorithm also has a notion of delayed KV writes and parallel KV writes- this translates cleanly to the new algorithm, where something that is delayed is just the next stage, and something that is parallel is just a step in the same stage.

We will also account for R2 writes: the runtime will just assume there is a stream/object body that already exists, and will just write to it.

We will follow the include tree to determine what values to write.

Because SAVE is so different, we should embrace two test files `select_tests.rs` and `save_tests.rs` to separate the two operations.

Additionally, once SAVE exists, we can go back and implement any inserts in the GET/LIST tests using the new SAVE operation, doubling down on the correctness of the SAVE operation.

# Final Steps

After GET/LIST is perfected and SAVE is implemented, we can now remove all of the old ORM code and replace it with the new query plan executor.

This change will span across the entire codebase: the compiler, the runtime, and the e2e tests. The compiler will need to be updated to generate the new query plan IR, the runtime need its own query executor that is rigorously tested for correctness (covering situations that aren't already covered in our mock executor), and the e2e tests will need to be updated to use the new query plan executor.
