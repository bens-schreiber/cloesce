To test the schema extractor out, run in the cloesce-ts directory:

- npm i --save-dev @types/node
- npm install cmd-ts
- npm install ts-morph
- npm run build
- npm link

Then go to whatever project you want to test in, and with node_modules created, run:

- npm link

Then to run the schema extractor run cloesce-ts --project-name (Whatever you want the project name to be)

Then you're done! Currently, the schema extractor will only process schemas with a naming scheme \*.cloesce.ts, so be careful with syntax.
\*\* Make sure that in your tsconfig you set `"strict": true` or else the schema extractor will assume nullable attributes are non-nullable.
