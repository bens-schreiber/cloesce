# Future Vision

Cloesce is an ambitious project that aims to create a simple but powerful paradigm for building full stack applications. Several goals drive the future development of Cloesce, shaping its design and evolution. Although still in its early stages, these goals provide a clear roadmap for its growth.

## Language Agnosticism

A central concept of Cloesce’s vision is full language agnosticism. Modern web development forces teams to navigate a maze of parallel ecosystems, each with its own web servers, dependency injection patterns, ORMs, migration tools, validation libraries, and routing conventions. These systems all solve the same problems, yet they do so with incompatible syntax and assumptions. Cloesce aims to collapse this redundancy by defining a single architecture that can be compiled into any target language, allowing developers to choose their preferred runtime without sacrificing consistency.

On the client side, this goal is well within reach: any language can consume REST APIs, making generated client code naturally portable. The server side, however, presents a deeper challenge. To move toward true language independence, Cloesce’s core is implemented in Rust and compiled to WebAssembly, enabling the possibility of targeting multiple server environments in the future.

The most significant obstacle today is the Cloudflare Workers platform, which treats only JavaScript and TypeScript as first-class citizens. Encouragingly, Cloudflare’s ongoing effort to support Rust based Workers through WebAssembly is now in beta and shows strong potential. As this matures, Cloesce will be able to target Rust and possibly other compiled languages without compromising its architecture.

Equally important is the evolution of WASI. With the upcoming WASI Preview 3 introducing a native async runtime, WebAssembly is rapidly becoming a viable foundation for general purpose server development. This progress directly expands the horizons of what Cloesce can support in the future, and also allows Cloesce to move the entirety of its runtime into WebAssembly, further decoupling it from any single language or platform.

## Support the full set of Cloudflare Workers features

Cloesce should be the best way to develop with Cloudflare. To achieve this, developers cannot be limited in leveraging the capabilities of the Workers platform. Future versions of Cloesce will aim to enhance the full range of Workers features, including:

- Durable Objects and Web Socket API generation
- Native solution for D1 sharding
- Worker to Worker communication
- Hyperdrive + PostgreSQL support
- ... and more as Cloudflare continues to expand the Workers platform.

## Designed for AI Integration

With Cloesce, you write less code. Significantly less. Furthermore, the code you do write is at a high level of abstraction, focusing on only data Models and business logic. This design makes Cloesce an ideal candidate for AI assisted development. Not only would creating a project require a fraction of the tokens, but the high level nature of the code means that AI can more easily understand the intent and structure of the application.

By its stable release, simply asking an AI agent to "Create Twitter with Cloesce" should yield a complete, functional application ready to deploy to Cloudflare Worker in a record low token count.