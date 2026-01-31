# Future Vision

Cloesce is an ambitious project with a vision to create a seamless and simple paradigm for building full stack applications. Several core goals drive the future development of Cloesce, and are responsible for many of the design decisions made so far. Although Cloesce is still in its early stages, these goals provide a roadmap for its evolution.

## Language Agnosticism

A central concept of Cloesce’s vision is full language agnosticism. Modern web development forces teams to navigate a maze of parallel ecosystems, each with its own web servers, dependency injection patterns, ORMs, migration tools, validation libraries, and routing conventions. These systems all solve the same problems, yet they do so with incompatible syntax and assumptions. Cloesce aims to collapse this redundancy by defining a single architecture that can be compiled into any target language, allowing developers to choose their preferred runtime without sacrificing consistency.

On the client side, this goal is already within reach: any language can consume REST APIs, making generated client code naturally portable. The server side, however, presents a deeper challenge. To move toward true language independence, Cloesce’s core is implemented in Rust and compiled to WebAssembly, enabling the possibility of targeting multiple server environments in the future.

The most significant obstacle today is the Cloudflare Workers platform, which treats only JavaScript and TypeScript as first-class citizens. Encouragingly, Cloudflare’s ongoing effort to support Rust based Workers through WebAssembly is now in beta and shows strong potential. As this matures, Cloesce will be able to target Rust and possibly other compiled languages without compromising its architecture.

Equally important is the evolution of WASI. With the upcoming WASI Preview 3 introducing a native async runtime, WebAssembly is rapidly becoming a viable foundation for general purpose server development. This progress directly expands the horizons of what Cloesce can support in the future.

## Support the full set of Cloudflare Workers features

Cloesce is intended to be the easiest, fastest and most productive way to build applications on Cloudflare Workers. To achieve this, Cloesce must not limit developers from leveraging the capabilities of the Workers platform. Future versions of Cloesce will aim to enhance the full range of Workers features, including:
- Durable Objects and Web Socket API generation
- Native D1 sharding and multi-database support
- Worker to Worker communication
- Hyperdrive support
- Queues
- ... and more as Cloudflare continues to expand the Workers platform.

## Designed for AI Integration

Since Cloesce compiles a significant portion of an applications code, it is uniquely positioned to integrate with AI tools. Measuring the capabilities and token usage of AI agents when directed to use alternative frameworks versus with Cloesce could reveal substantial efficiency gains. As Cloesce progresses, so should the ability to open up a new AI agent and have it build a complete full stack application with minimal prompting-- maybe even just a simple description of the desired functionality.