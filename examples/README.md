# Example Cloesce TypeScript Project v0.0.2

v0.0.2 is a super bare bones version of Cloesce. To get a project running:

1. Run extractor:

- Go into `src/extractor/ts` and `npm run build`, ensuring that `src/extractor/ts/dist/cli.js` is executable.
- Run `cloesce` in `examples` to generate the CIDL

2. Run generators

```bash
cd src/generator

cargo run generate all \
  ../../examples/.generated/cidl.json \
  ../../examples/wrangler.toml \
  ../../examples/.generated/migrations/seed.sql \
  ../../examples/.generated/workers.ts \
  ../../examples/.generated/client.ts \
  http://localhost:5173/api \
  http://localhost:5000/api 

```

3. Wrangle' it

```bash
# migrate wrangler
cd ../../examples/
echo y | npx wrangler d1 migrations apply example

# build
npx wrangler build

# run wrangler
npx wrangler dev --port 5000
```

4. Run frontend
- `npm run frontend`