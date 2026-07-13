# Model Catalog

The curated catalog is versioned data, not executable code. Each release has a schema version, catalog version, creation time, signing key ID, detached signature, and explicit model assets.

Entries include identity, family, task, exact asset URL, filename, format, quantization, parameter count, context length, byte size, SHA-256, license, tags, backend compatibility constraints, memory estimates, projector requirements, and runtime version bounds.

Catalog refresh is user-controlled and may use the network only when internet metadata access is enabled. A failed signature keeps the last verified catalog. Model repositories are never cloned and remote Python or JavaScript is never executed.

Recommendations combine catalog compatibility with the local capability matrix and measured benchmarks as described in `HARDWARE_ACCELERATION.md`. Badges are estimates and expose their confidence. Locally imported models remain usable without a catalog entry but begin with conservative compatibility and require validation.
