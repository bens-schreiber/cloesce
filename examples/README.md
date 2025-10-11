# Example Cloesce TypeScript Project v0.0.2

v0.0.3 is the official "launch" of Cloesce as npm now orchestrates
the TypeScript and Rust logic and the package is now released onto npm

1. Install npm package (do this in the examples dir):
- Run `npm install`

2. Run npm package:
- cloesce run

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