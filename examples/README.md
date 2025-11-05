# Example Cloesce TypeScript Project v0.0.3

v0.0.3 is a bare bones pre-alpha version of cloesce.

1. Install npm package (do this in the examples dir):

- Run `npm install`

2. Compile cloesce

- `npx cloesce compile`

3. Migrate

- `npx cloesce migrate Initial`

4. Wrangle' it

```bash
# migrate wrangler
echo y | npx wrangler d1 migrations apply example

# run wrangler
npx wrangler dev --port 5000
```

5. Run frontend

- `npm run frontend`
