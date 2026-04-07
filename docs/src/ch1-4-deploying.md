# Deploying

With your application built and your database migrated, you're ready to deploy your Cloesce application to Cloudflare Workers. Deployment is done through the Wrangler CLI.

1. **Modify `cloesce.jsonc`**
   
   Ensure your `cloesce.jsonc` file is correctly configured for production, including the production Worker URL.

2. **Configure Wrangler bindings**
   
   Open your `wrangler.jsonc` and set all required binding IDs (e.g., `kv_namespaces`, `d1_databases`, `r2_buckets`) to their production values.

   ```jsonc
   {
      "r2_buckets": [
         {
            "binding": "bucket",
            "bucket_name": "xxxxxxxx"
         }
      ],
   }
   ```

3. **Build your application**
   
   Run the compile command to generate the necessary files for deployment:

   ```bash
   npx cloesce compile
   ```

4. **Deploy using Wrangler**
   
   Publish your application to Cloudflare Workers:

   ```bash
   npx wrangler deploy
   ```

5. **Deploy your frontend**
   
   If you have a frontend application (e.g., built with Vite), build and deploy it to your preferred hosting service. For example, with [Cloudflare Pages](https://pages.cloudflare.com):

   ```bash
   npx wrangler pages deploy ./dist
   ```


