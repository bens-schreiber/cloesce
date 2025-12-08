# Sprint 3 Report (v0.0.5)
*(11-5-2025 to 12-5-2025)*

## What's New (User Facing)

* Introduced **Cloesce services** to streamline entity-first workflows, enabling automatic generation of Wrangler-ready TypeScript service layers.
* Added first-class **blob type** support, including binary persistence, metadata handling, and transport-safe serialization for edge runtimes.
* Implemented **octet stream processing** across the platform, supporting efficient ingestion, transformation, and delivery of arbitrary byte streams.
* Integrated **R2-backed storage** with unified access primitives and model-aware binding generation for static assets and binary data.
* Lots and lots of bug fixes
---

## Work Summary (Developer Facing)

This sprint focused on solidifying the core architecture required to make Cloesce a production-ready entity-first compiler and runtime ecosystem.
We advanced the platform by introducing Cloesce services, fully decoupling business logic from model compilation and enabling instant emission of Wrangler-ready TypeScript artifacts. Structural hashing was leveraged to track CIDL-level model changes across builds, ensuring the compiler could reliably detect and reconcile schema-relevant updates. The team also expanded the runtime pipeline with support for blob types and efficient octet-stream handling, allowing binary data to flow through generated services without format inconsistencies.
R2 integration was completed, providing unified storage primitives and automatic binding generation across all compiled outputs. Work continued on refining the entity-first compiler so TypeScript models produce complete handlers and deployment files with minimal configuration.

---

## Unfinished Work

The remaining R2 integration still requires full bucket-level support, including creation, listing, namespacing, and lifecycle operations to complete the storage layer.
We also need to address slow compilation times, which have become more noticeable as the entity-first pipeline grows. Improvements to type analysis, merkle tree compilation, code emission, and incremental rebuilds are planned to restore efficient compilation performance.
---

## Completed Issues/User Stories
- [#126](https://github.com/bens-schreiber/cloesce/issues/126)  
- [#125](https://github.com/bens-schreiber/cloesce/issues/125)  
- [#118](https://github.com/bens-schreiber/cloesce/issues/118)  
- [#113](https://github.com/bens-schreiber/cloesce/issues/113)  
- [#111](https://github.com/bens-schreiber/cloesce/issues/111)  
- [#106](https://github.com/bens-schreiber/cloesce/issues/106)  
- [#99](https://github.com/bens-schreiber/cloesce/issues/99)
---

## Incomplete Issues/User Stories

Some work from this sprint could not be completed due to unexpected technical complexities:

1. **[#115 – Migrations Issues Dump](https://github.com/bens-schreiber/cloesce/issues/115):** 
This work was delayed because several edge-case failures surfaced late in testing, requiring deeper inspection of the migration planner’s structural hashing logic than originally estimated. 

2. **[#127 – R2 Support](https://github.com/bens-schreiber/cloesce/issues/127):**  
Full R2 bucket support was held back because the underlying binding generator needed a redesign to accommodate multi-bucket environments.

3. **[#51 – Add Arrays/Objects to GET Requests](https://github.com/bens-schreiber/cloesce/issues/51):**  
This feature was sent to the backlog after we discovered that the current routing layer could not reliably serialize nested collection types without introducing breaking changes to existing handlers.

---

## Code Files for Review

Most files in the [Cloesce Repository](https://github.com/bens-schreiber/cloesce) were modified due to major refactoring.

---

## Retrospective Summary

### What Went Well
- **Cloesce services** clarified the service layer and made generated Wrangler outputs feel production-ready.  
- **Blob types and octet-stream support** broadened Cloesce’s data capabilities and enabled real binary workflows.  
- Initial **R2 integration** proved the compiler can target functional cloud storage backends.  
- **Semantic analysis with structural hashing** ensured accurate model-change detection across builds.

### Areas for Improvement
- Better **documentation of progress** for Capstone tracking.
- Integrate more bug fixes into sprint plans to avoid bugs collecting over time.

### Next Sprint Plans
- More bug fixes and extended testing  
- R2 buckets completion 
- Finish adding arrays/objects to GET requests
- Add multiple databases to be used
- Add indexing and composite keys  
- Add sugaring for better user experience 