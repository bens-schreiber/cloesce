import { D1Database } from "@cloudflare/workers-types";

import { Person } from "../models/person.cloesce";

export interface Env {
  DB: D1Database;
}

function match(
  router: any,
  path: string,
  request: Request,
  env: any
): Response {
  let node: any = router;
  const params: any[] = [];
  const segments = path.split("/").filter(Boolean);
  for (const segment of segments) {
    if (node[segment]) {
      node = node[segment];
    } else {
      const paramKey = Object.keys(node).find(
        (k) => k.startsWith("<") && k.endsWith(">")
      );
      if (paramKey) {
        params.push(segment);
        node = node[paramKey];
      } else {
        return new Response(
          JSON.stringify({ error: "Route not found", path }),
          {
            status: 404,
            headers: { "Content-Type": "application/json" },
          }
        );
      }
    }
  }
  if (typeof node === "function") {
    return node(...params, request, env);
  }
  return new Response(JSON.stringify({ error: "Route not found", path }), {
    status: 404,
    headers: { "Content-Type": "application/json" },
  });
}

const router = {
  api: {
    Person: {
      "<id>": {
        speak: async (id: number, request: Request, env: any) => {
          if (request.method !== "GET") {
            return new Response("Method Not Allowed", { status: 405 });
          }

          let body: any;
          try {
            body = await request.json();
          } catch {
            return new Response(
              JSON.stringify({ error: "Invalid request body" }),
              {
                status: 400,
                headers: { "Content-Type": "application/json" },
              }
            );
          }

          const { count } = body;

          if (count === null || count === undefined) {
            throw new Error("Required parameter missing: count");
          }
          if (count !== null && typeof count !== "number") {
            throw new Error("Parameter count must be a number");
          }

          const d1 = env.D1_DB || env.DB;

          const query = `SELECT * FROM Person WHERE id = ?`;
          const record = await d1.prepare(query).bind(id).first();

          if (!record) {
            return new Response(JSON.stringify({ error: "Record not found" }), {
              status: 404,
              headers: { "Content-Type": "application/json" },
            });
          }

          const instance: Person = { id, name, ssn };

          return instance.speak(count);
        },
      },
      post: async (request: Request, env: any) => {
        if (request.method !== "POST") {
          return new Response("Method Not Allowed", { status: 405 });
        }

        let body: any;
        try {
          body = await request.json();
        } catch {
          return new Response(
            JSON.stringify({ error: "Invalid request body" }),
            {
              status: 400,
              headers: { "Content-Type": "application/json" },
            }
          );
        }

        const { name, ssn } = body;

        if (name === null || name === undefined) {
          throw new Error("Required parameter missing: name");
        }
        if (name !== null && typeof name !== "string") {
          throw new Error("Parameter name must be a string");
        }
        if (ssn !== null && typeof ssn !== "string") {
          throw new Error("Parameter ssn must be a string");
        }

        return Person.post(name, ssn);
      },
    },
  },
};

export default {
  async fetch(request: Request, env: any, ctx: any): Promise<Response> {
    try {
      const url = new URL(request.url);
      return await match(router, url.pathname, request, env);
    } catch (error: any) {
      console.error("Internal server error:", error);
      return new Response(JSON.stringify({ error: error?.message }), {
        status: 500,
        headers: { "Content-Type": "application/json" },
      });
    }
  },
};
