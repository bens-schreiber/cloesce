# Installation

## Installing the Compiler

**Linux and macOS**

```sh
curl -fsSL https://cloesce.pages.dev/install.sh | sh
```

**Windows (PowerShell)**

```powershell
irm https://cloesce.pages.dev/install.ps1 | iex
```

Then verify the installation:

```sh
cloesce version
```

## Starting a New Project

The fastest way to get a Cloesce project up and running is to use the `create-cloesce` template.

### Prerequisites

1. Sign up for a [Cloudflare account](https://dash.cloudflare.com/sign-up/workers-and-pages) (_not necessary for local development_)
2. Install [Node.js](https://nodejs.org/) (version `16.17.0` or later)

### create-cloesce

Run the following command in your terminal:

```bash
npx create-cloesce my-cloesce-app
```

After running the command, navigate into your new project directory:

```bash
cd my-cloesce-app
```

A simple project structure is created for you.

```
├── src/
│   ├── api/            # API route handlers
│   ├── web/            # Frontend web assets
│   └── schema/
│       └── schema.clo  # Cloesce schema definition
├── test/               # Unit tests for example Models
├── migrations/         # Database migration files
├── cloesce.jsonc       # Cloesce configuration
└── package.json
```
