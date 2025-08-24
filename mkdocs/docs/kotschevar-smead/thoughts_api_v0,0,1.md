# Cloesce → Cloudflare Serverless REST API (Thought Dump)

## Goal
- Generate a **Cloudflare Workers REST API** from the Cloesce IDL.  
- Provide **auto CRUD** endpoints for each model.  
- Allow **static** and **instance** methods to be defined in the schema.  
- Let methods **override** CRUD routes when needed.  

## Routing & Conventions

### CRUD defaults
- **Collection routes**
  - `GET /person` → list with pagination/filters
  - `POST /person` → create a new entry
- **Item routes**
  - `GET /person/{id}` → fetch by ID
  - `PATCH /person/{id}` → update
  - `DELETE /person/{id}` → delete (soft-delete optional)

### Methods
- **Static**: `/person/{method}`  
  - e.g. `GET /person/averageage`  
  - e.g. `POST /person/reindex`
- **Instance**: `/person/{id}/{method}`  
  - e.g. `POST /person/{id}/disable`  
  - e.g. `POST /person/{id}/promote`
- **Overrides**
  - Explicitly declared method named `get` could replace `GET /person`.
  - Otherwise methods extend CRUD, not shadow them.

---

## Schema Extensions

Each model may declare an `api` section with methods.

- **Static methods**: apply to the whole collection.  
- **Instance methods**: apply to a single entity.  

Each method can define:
- Route name  
- HTTP verb (default: `POST`)  
- Optional **override flag** to replace CRUD  
- Auth/roles  
- Input/output shapes  

### Example Schema Snippet
```json
{
  "cidl_version": "0.0.1",
  "models": {
    "Person": {
      "columns": {
        "id": { "type": 0, "primary_key": true },
        "name": { "type": 1, "nullable": false },
        "isActive": { "type": 4, "default": true }
      },
      "methods": {
        "static": {
          "search": {
            "http": { "verb": "GET" },
            "query": { "q": { "type": "string", "required": true } }
          },
          "averageage": {
            "http": { "verb": "GET" },
            "returns": { "type": "number" }
          },
          "reindex": {
            "http": { "verb": "POST" },
            "roles": ["admin"]
          }
        },
        "instance": {
          "disable": {
            "http": { "verb": "POST" },
            "roles": ["admin","support"]
          },
          "rename": {
            "http": { "verb": "POST" },
            "body": { "name": { "type": "string" } }
          },
          "promote": {
            "http": { "verb": "POST" },
            "roles": ["admin"],
            "body": {
              "newRole": {
                "type": "string",
                "enum": ["manager","director","vp"],
                "required": true
              }
            },
            "returns": {
              "type": "object",
              "properties": {
                "id": { "type": "integer" },
                "name": { "type": "string" },
                "role": { "type": "string" }
              }
            }
          }
        }
      }
    }
  }
}
```

## Design Principles

- **CRUD by default** → every model is usable immediately.
- **Explicit methods** → schema controls what's exposed.
- **Safe overrides** → CRUD only replaced if explicitly declared.
- **Predictability** → static = /model/method, instance = /model/{id}/method.
- **Consistency** → all models follow the same REST patterns.

## Generated CRUD API

For Person:

### CRUD
- `GET /person`
- `POST /person`
- `GET /person/{id}`
- `PATCH /person/{id}`
- `DELETE /person/{id}`

### Methods
- `GET /person/search?q=ben`
- `GET /person/averageage`
- `POST /person/{id}/disable`
- `POST /person/{id}/rename`
- `POST /person/{id}/promote`


## Code Generation Shape (Language-Agnostic)

```typescript
// Standardized error types for generation
enum APIErrorType {
  ValidationError = "ValidationError",
  BusinessLogicError = "BusinessLogicError", 
  AuthorizationError = "AuthorizationError",
  NotFoundError = "NotFoundError",
  ConflictError = "ConflictError",
  DatabaseError = "DatabaseError",
  InternalError = "InternalError"
}

// Generated error responses
interface ErrorResponse {
  error: APIErrorType;
  message: string;
  details?: any[];
  timestamp: string;
  requestId: string;
}
```

### Handler Lifecycle
1. Validate input.
2. Execute:
   - CRUD → repository helper
   - Method → invoke generated stub
3. Return JSON response with status codes.

## Instance Method Execution Plan

**Example**: `POST /person/{id}/promote`

###  Pseudocode
```python

#Extract data
try:
    id = extractAndValidateId(request.params.id)
    modelName = request.route.model  # "Person"
    methodName = request.route.method  # "promote"
except ValidationError as e:
    return { status: 400, error: "InvalidRequest", details: e.errors }


#Start Transaction
tx = db.beginTransaction()
try:
    #  Fetch & Validate Entity Exists
    row = tx.query("SELECT * FROM Person WHERE id = ?", [id])
    if not row:
        tx.rollback()
        return { status: 404, error: "NotFound", resource: f"Person/{id}" }
    
    #  Map to row to Model
    person = mapRowToModel("Person", row)
    

    #  Execute Business Logic
    result, mutatedState = person.promote(input)
    
    #  Handle Method Response Types
    if result.isError():
        tx.rollback()
        return { 
            status: result.statusCode || 422, 
            error: result.errorType || "BusinessLogicError",
            message: result.message 
        }
    
    #  Persist Changes (if mutation occurred)
    if mutatedState:
        updateFields = extractChangedFields(person, mutatedState)
        if updateFields.length > 0:
            tx.exec(
                buildUpdateQuery("Person", updateFields, ["id"]), 
                [...updateFields.values, id]
            )
    
    #  Commit Transaction
    tx.commit()
    
    # Return Appropriate Response
    if result.hasPayload():
        return { status: 200, data: result.payload }
    elif mutatedState:
        return { status: 200, data: mapModelToResponse(mutatedState) }
    else:
        return { status: 200, data: { ok: true, message: result.message } }

except DatabaseError as e:
    tx.rollback()
    logError("DatabaseError", e, { modelName, methodName, id })
    return { status: 500, error: "InternalError", message: "Database operation failed" }
    
except Exception as e:
    tx.rollback()
    logError("UnexpectedError", e, { modelName, methodName, id })
    return { status: 500, error: "InternalError", message: "Unexpected error occurred" }
```

## Static Method Execution Plan

**Example**: `GET /person/averageage`

1. Match route → model Person, method averageage.
2. Validate query params.
3. Call Methods.Person.averageage(input, context).
4. Return payload.

