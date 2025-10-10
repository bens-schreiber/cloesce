# Example Cloesce TypeScript Project v0.0.2

v0.0.3 is the official "launch" of Cloesce as npm now orchestrates
the TypeScript and Rust logic and the package is now released onto npm

1. Run extractor:
- Run `npm install cloesce`
- Make a default cloesce-config.json in the root project directory

Ex:
{
  "source": "./src",
  "workersUrl": "http://localhost:5002/api",
  "clientUrl": "http://localhost:5002/api"
}

2. Run generators
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