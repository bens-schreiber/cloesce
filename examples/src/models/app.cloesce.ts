import { CloesceApp } from "cloesce/backend";

const app = new CloesceApp();

app.onResponse(async (request, env, di, response: Response) => {
  // basic CORS, allow all origins
  response.headers.set("Access-Control-Allow-Origin", "*");
  response.headers.set(
    "Access-Control-Allow-Methods",
    "GET, POST, PUT, DELETE, OPTIONS"
  );
  response.headers.set(
    "Access-Control-Allow-Headers",
    "Content-Type, Authorization"
  );
});

export default app;
