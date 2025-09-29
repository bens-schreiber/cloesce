# Cloesce Middleware Strategy



Use a middleware registry. Middleware defined once, referenced by name in decorators.

---

## How It Works

```mermaid
TODO create image
```

### 1. Developer Defines Middleware

```typescript
// middleware.ts
export const middlewares = {
  cors: {
    type: "cors",
    allowOrigins: ["https://example.com"]
  },
  
  rateLimit: {
    type: "rateLimit",
    maxRequests: 100,
    windowMs: 60000
  },
  
  jwtAuth: {
    type: "jwt",
    secret: "${JWT_SECRET}"
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
    jwtAuth();
    RateLimit();
    //Rest of function ..
    
}

async function getPublicInfo(req: Request, env: Env) {
    // If the "cors" middleware is registered, it will call its function
    
    //Rest of function
    
}
```


```

---

## Implementation Plan

### Phase 1: Core (Week 1-2)
- Middleware type system
- Extractor parses middleware.ts and @Middleware decorators
- CIDL schema update
- Generator creates registries

### Phase 2: Built-ins (Week 3)
- CORS middleware
- Rate limiting (KV-based)
- JWT auth
- API key auth

### Phase 3: Runtime (Week 4)
- Update cloesce() to handle middleware registry
- Execute middleware chains before endpoint handlers
- Early return on middleware blocking

### Phase 4: DX (Week 5)
- TypeScript types
- CLI validation
- Docs and examples

---

## Design Decisions

**Decorator vs File-based**: Decorators keep middleware close to code, better for model-first paradigm

**Execution Order**: Global (model) → Local (method), always predictable

**Early Returns**: Middleware returns Response to block, nothing to continue

**Context Passing**: Use context object with `locals` for shared state between middleware

```typescript
interface MiddlewareContext {
  request: Request;
  env: Env;
  next: () => Promise<Response>;
  locals?: Record<string, any>;
}
```

---

## Inspiration
**OpenAPI Security Patterns**:
- [Security Authentication](https://swagger.io/docs/specification/v3_0/authentication/)
- [OAS Tools Security](https://oas-tools.github.io/docs/features/security)
