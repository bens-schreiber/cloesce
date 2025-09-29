# Cloesce Middleware Strategy



Use a middleware registry. Middleware defined once, referenced by name in decorators.

---

## How It Works


### 0. Middleware Registery could have
```typescript
{
   // middleware/index.ts
export const middlewares = {
  cors: {
    source: "./cors.ts",
    function: "corsMiddleware"
  },
  
  rateLimit: {
    source: "./rateLimit.ts",
    function: "rateLimitMiddleware"
  },
  
  jwtAuth: {
    source: "../auth/jwt.ts",
    function: "jwtAuthMiddleware"
  }
};
    
    
}
```


### 1. Developer Defines Middleware



```typescript
// middleware.ts
export const middlewares = {
  cors: {
    type:cors
    function:corsMiddleware
    source./
  },
  
  rateLimit: {
    type: "rateLimit",
    function:rateLimitMiddleware
    source:./
  },
  
  jwtAuth: {
    type: "jwt",
    function:jwtAuthMiddleware
    source:./
  }
};

export async function corsMiddleware(req: Request, env: Env) {
  if (req.method === 'OPTIONS') {
    return new Response(null, {
      headers: {
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE'
      }
    });
  }
}

export async function rateLimitMiddleware(req: Request, env: Env) {
  const ip = req.headers.get('CF-Connecting-IP');
  const count = await env.RATE_LIMIT_KV.get(`ratelimit:${ip}`);
  
  if (count && parseInt(count) > 100) {
    return new Response('Too Many Requests', { status: 429 });
  }
  
  await env.RATE_LIMIT_KV.put(`ratelimit:${ip}`, (parseInt(count || '0') + 1).toString(), {
    expirationTtl: 60
  });
}

export async function jwtAuthMiddleware(req: Request, env: Env) {
  const token = req.headers.get('Authorization')?.replace('Bearer ', '');
  if (!token) {
    return new Response('Unauthorized', { status: 401 });
  }
  
  const isValid = await verifyJWT(token, env.JWT_SECRET);
  if (!isValid) {
    return new Response('Forbidden', { status: 403 });
  }
}
```

### 2. Apply to Models
(Via Decorator)
```typescript
@Middleware(["cors"])  // Applied to all endpoints
@D1
class Person {
    id: number;
    name: string;

    @Workers.GET
    async getPublicInfo(db: D1Db) {
      // Just inherits cors
    }

    @Workers.POST
    @Middleware(["jwtAuth", "rateLimit"])  // Adds these on top of cors
    async updatePerson(db: D1Db, data: PersonUpdate) {
      // Gets: cors -> jwtAuth -> rateLimit
    }
}
```

### 3. Extractor Creates CIDL

```json
{
  "middleware": {
    "definitions": [
      {
        "name": "cors",
        "source_path": "/project/src/middleware.ts", //Could be any file
        "function_name": "corsMiddleware"
      },
      {
        "name": "jwtAuth",
        "source_path": "/project/src/middleware.ts",
        "function_name": "jwtAuthMiddleware"
      },
      {
        "name": "rateLimit",
        "source_path": "/project/src/middleware.ts",
        "function_name": "rateLimitMiddleware"
      }
    ]
  },
  "models": [
    {
      "name": "Person",
      "middleware": ["cors"],
      "methods": [
        {
          "name": "getPublicInfo",
          "http_verb": "GET",
          "middleware": []
        },
        {
          "name": "updatePerson",
          "http_verb": "POST",
          "middleware": ["jwtAuth", "rateLimit"]
        }
      ]
    }
  ]
}
```

### 4. Generated Worker

```typescript
import { cloesce } from "cloesce";
import cidl from "./cidl.json" with { type: "json" };
import { Person } from "./src/models/Person";
import { corsMiddleware, rateLimitMiddleware, jwtAuthMiddleware } from "./src/middleware";

const constructorRegistry = {
	Person: Person
};

const middlewareRegistry = {
	corsMiddleware: corsMiddleware,
	rateLimitMiddleware: rateLimitMiddleware,
	jwtAuthMiddleware: jwtAuthMiddleware
};

export default {
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {
        const instanceRegistry = new Map([["Env", env]]);

        return await cloesce(
            cidl, 
            constructorRegistry, 
            middlewareRegistry,
            instanceRegistry, 
            request, 
            "/api", 
            { envName: "Env", dbName: "DB" }
        );
    }
};
```



```typescript
// Example: Function using @Middleware decorator to include CORS

async function UpdatePerson(req: Request, env: Env) {
    // If the middleware is registered, it will call its function
    cors();
    jwtAuth();
    RateLimit();
    //Rest of function ..
    
}

async function getPublicInfo(req: Request, env: Env) {
    // If the "cors" middleware is registered, it will call its function
    cors();
    //Rest of function
    
}
```

## Inspiration
**OpenAPI Security Patterns**:
- [Security Authentication](https://swagger.io/docs/specification/v3_0/authentication/)
- [OAS Tools Security](https://oas-tools.github.io/docs/features/security)
