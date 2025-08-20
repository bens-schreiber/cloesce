# Cloesce Abstract

## Problem Statement

Web development (being, development utilizing an HTTP server) is full of repetitive, boilerplate code. To create a simple CRUD application, a developer would need to:

1. Create a database schema
2. Create CRUD API endpoints for their database tables
3. Fill those API endpoints with basic CRUD SQL calls
4. Create a frontend API capable of calling those API endpoints
5. Host this on some relevant infrastructure (as of 2025, a cloud provider such as Cloudflare or AWS)

A consumer facing CRUD application would also need authentication, likely through fine grained role-based permissions.

This elaborate process, spanning several files across hundreds of lines of code to create what a consumer would see as a trivial application has improved over time. Instead of seeing these five criterion as entirely seperate entities, new paradigms and libraries released with the objective of doing more with less. Notably, these are:

- Swagger
- Remote Procedure Calls (gRPC, Cap'n Proto)
- GraphQL
- IntelliTect's Coalesce

Swagger defines a contract generated from the backend API endpoints, such that the frontend can have a simple interface to call these methods (without manually writing out endpoints). Importantly, Swagger will also copy models from the backend to the frontend, so developers do not need to redefine types.

RPC calls (in the context of web development) have the goal of creating a unified API for both the backend and the frontend. Unlike Swagger, RPC aims to do this transparently-- the developer should not know that this call induces a network request. Typically, RPC communicates over their own more efficient channels not suited for basic REST calls.

GraphQL adds another step, unifying the database, backend, and frontend (by almost eliminating the backend), allowing the business logic to sit on the frontend. Because of this, GraphQL has several flaws and has generally fallen out of use in the web development community.

Finally, IntelliTect's Coalesce unites the database, backend, and frontend utilizing Microsoft's .NET Entity Framework. A developer needs only to define a C# model, which can then be generated into a SQL database, a backend REST API, a frontend API, and also frontend Vue.js components to easily interact with the service. The paradigms Coalesce uses will be critical in designing Cloesce.

What is interesting about these technologies is none of them are natively capable of covering all web development criterion: database, backend, frontend and cloud infrastructure. And although Coalesce gets close, it does not stay in the spirit of Swagger, RPC and GraphQL as it requires that you use the .NET environment-- it is not language agnostic. This is where Cloesce comes in to play.

## Why

Let's now imagine we have a framework capable of generating all relevant of a web development project including cloud infrastructure from individual models (much like how RPC defines a shared interface). Cloesce aims to do this natively from your code: be it in TypeScript, Python, Rust, etc. What would the developer gain?

The logic for the entire application would sit directly on a model. We aim to add a layer of transparency: you define a model, and methods on that model. Although the actual content of a model exists in a seperate SQL database, the methods on that model are oblivious to that fact, and can operate as if they exist in memory on the same computer. Continuing with this transparency, a developer should not need to think of the final cloud environment a model lives on as a seperate entity, but only decide how that model should be called (be it through a serverless lambda, or a long lived container). Importantly, from the perspective of the frontend, the developer should be able to call a model as if it exists on it's own machine, even though it will require a network call. With this in mind, we now have one central area of business logic that dictates and generates how the application can be used-- developers are no longer focusing on the "glue", but rather the design of the system.

Another important gain of this paradigm is security. Every time a developer has to write their own boilerplate for delicate areas of code, they risk introducing vulnerabilities into the system. If the majority of this can be deterministically generated, the security can be verified from the compiler.

Finally, in the age of LLM's producing most code we should ask ourselves: do I want the LLM to write _more_ code? Spinning up the latest OpenAI model and asking it to "make me a full stack TODO app deployed to Azure" is almost sure to fail, because the task is far too general. On top of this, asking a question as general as that would burn through tokens. An LLM would benefit the same way developers would from a central area for logic-- all it needs to do is write a model, run a generate command, and make a pretty frontend.

## How

Cloesce is essentially a compiler: given a native language input, create an intermediate representation such that we can generate the appropriate outputs from a secondary generator

![Cloesce Compiler Drawio Diagram](./assets/abstract-how-compiler-diagram.png)
