# Final Steps

After GET/LIST is perfected and SAVE is implemented, we can now remove all of the old ORM code and replace it with the new query plan executor.

This change will span across the entire codebase: the compiler, the runtime, and the e2e tests. The compiler will need to be updated to generate the new query plan IR, the runtime need its own query executor that is rigorously tested for correctness (covering situations that aren't already covered in our mock executor), and the e2e tests will need to be updated to use the new query plan executor.
