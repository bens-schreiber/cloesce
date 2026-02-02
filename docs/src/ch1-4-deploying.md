# Deploying

> *Alpha Note*: Deployment is not yet enhanced by Cloesce and has room for improvement.

With your application built and your database migrated, you're ready to deploy your Cloesce application to Cloudflare Workers. Deployment is done through the Wrangler CLI.

1. **Modify `cloesce.config.json`**
   
   Ensure your `cloesce.config.json` file is correctly configured for production, including the production Workers URL.

   > NOTE: Workers URLs must have some path component (e.g., `https://my-app.workers.dev/api` has `/api`).

2. **Configure Wrangler bindings**
   
   Open your `wrangler.toml` and set all required binding IDs (e.g., `kv_namespaces`, `d1_databases`, `r2_buckets`) to their production values.

   Example:

   ```toml
   [[kv_namespaces]]
   binding = "kv"
   id = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
   ```

3. **Build your application**
   
   Run the compile command to generate the necessary files for deployment:

   ```bash
   $ npx cloesce compile
   ```

4. **Deploy using Wrangler**
   
   Publish your application to Cloudflare Workers:

   ```bash
   $ npx wrangler deploy
   ```

5. **Deploy your frontend**
   
   If you have a frontend application (e.g., built with Vite), build and deploy it to your preferred hosting service. For example, with [Cloudflare Pages](https://pages.cloudflare.com)

   ```bash
   $ npx wrangler pages deploy ./dist
   ```


