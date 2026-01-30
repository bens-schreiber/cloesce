# Installation
The simplest way to get a Cloesce project up and running is to use the `create-cloesce` template. This template sets up a basic Cloesce project structure with all the necessary dependencies and configurations, basic example models, and unit tests to get you started quickly. The template includes a sample HTML frontend with Vite which should be replaced with your frontend of choice.

## Prerequisites

> *Note*: Cloesce supports only TypeScript to TypeScript compilation as of Alpha v0.1.0. Support for additional languages will be added in future releases.

Cloesce depends solely on Wrangler, Cloudflare's CLI tool. Make sure you have [Wrangler installed on your machine](https://developers.cloudflare.com/workers/wrangler/install-and-update/). You can install it via npm:

```bash
$ npm i -D wrangler@latest
```

## Using create-cloesce

To create a new Cloesce project using the `create-cloesce` template, run the following command in your terminal:

```bash
$ npx create-cloesce my-cloesce-app
```

After running this command, navigate into your new project directory:

```bash
$ cd my-cloesce-app
```

A simple project structure is set up for you, including example models and unit tests:
```
├── src/
│   ├── data/           # Example Cloesce models
│   └── web/            # Frontend web assets
├── tests/              # Unit tests for example models
├── migrations/         # Database migration files
├── cloesce.config.json # Cloesce compiler configuration
└── package.json        # Project dependencies and scripts
```