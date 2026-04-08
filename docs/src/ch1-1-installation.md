# Installation

> [!NOTE]
> Cloesce supports only TypeScript compilation as of v0.3.0. Support for additional languages will be added in future releases.

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

The simplest way to get a Cloesce project up and running is to use the `create-cloesce` template. 

This template sets up a basic Cloesce project structure with all the necessary dependencies, configurations, a basic schema, and example tests to help get you started quickly. The template includes a sample HTML frontend with Vite which should be replaced with your frontend of choice.

### Prerequisites

1. Sign up for a [Cloudflare account](https://dash.cloudflare.com/sign-up/workers-and-pages) (*not necessary for local development*)
2. Install [Node.js](https://nodejs.org/) (version `16.17.0` or later)


### create-cloesce

To create a new Cloesce project using the `create-cloesce` template, run the following command in your terminal:

```bash
npx create-cloesce my-cloesce-app
```

After running this command, navigate into your new project directory:

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
└── package.json        # Project dependencies and scripts
```