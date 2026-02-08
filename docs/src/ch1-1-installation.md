# Installation

> *Alpha Note*: Cloesce supports only TypeScript to TypeScript compilation as of Alpha v0.1.0. Support for additional languages will be added in future releases.

The simplest way to get a Cloesce project up and running is to use the `create-cloesce` template. 

This template sets up a basic Cloesce project structure with all the necessary dependencies, configurations, example Models, and example tests to help get you started quickly. The template includes a sample HTML frontend with Vite which should be replaced with your frontend of choice.

## Prerequisites

1. Sign up for a [Cloudflare account](https://dash.cloudflare.com/sign-up/workers-and-pages)
2. Install [Node.js](https://nodejs.org/) (version `16.17.0` or later)

> *Note*: An account is only necessary if you plan to deploy your Cloesce application. Local development and testing can be done without an account.

## create-cloesce

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
│   ├── data/           # Example Cloesce Models
│   └── web/            # Frontend web assets
├── test/               # Unit tests for example Models
├── migrations/         # Database migration files
├── cloesce.config.json # Cloesce compiler configuration
└── package.json        # Project dependencies and scripts
```