# Proposal: Durable Objects

- **Author(s):** Ben Schreiber
- **Status:** **Draft** | Review | Accepted | Rejected | Implemented
- **Created:** 2026-04-26
- **Last Updated:** 2026-04-26

---

## Summary

Durable Objects are a Cloudflare Workers primitive that provide a way to model "stateful objects" in a distributed environment. Importantly, they enable a database-per-object model, and are capable of maintaining web socket connections. 