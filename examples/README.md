# Example Cloesce TypeScript Project v0.0.4

1. Install npm package (in the examples dir):

- Run `npm install`

2. Compile cloesce

- `npx cloesce compile`

3. Wrangle' it

```bash
# migrate wrangler
echo y | npx wrangler d1 migrations apply example

# run wrangler
npx wrangler dev --port 5000
```

4. Run frontend

- `npm run frontend`
