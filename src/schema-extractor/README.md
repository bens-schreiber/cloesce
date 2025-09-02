To test the schema extractor out, run in the cloesce-ts directory:
- npm i --save-dev @types/node
- npm run build
- npm link

Then go to whatever project you want to test in, and with node_modules created, run:
- npm link

Then to run the schema extractor run cloesce-ts --project-name (Whatever you want the project name to be)

Then you're done! Currently, the schema extractor will only process schemas in models-cloesce, with a naming 
scheme *.cloesce.ts, so be careful with syntax. 