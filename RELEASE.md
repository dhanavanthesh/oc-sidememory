<!-- Portions derived from outlines-core and modified by OC-Sidememory contributors. -->
<!-- See PROVENANCE.md and MODIFICATIONS.md. -->

# Release procedure

Publishing requires explicit maintainer authorization and a clean commit whose version matches the proposed tag.

1. Run the final Rust and Python gates.
2. Inspect and build the Cargo archive.
3. Build wheel and source-distribution artifacts.
4. Build a wheel from the source distribution.
5. Install artifacts into clean environments.
6. Run semantic smoke checks and examples.
7. Verify platform tags, package contents, provenance, and licenses.
8. Generate SHA-256 hashes and a versioned manifest.

The dry run stops before registry upload. Recheck registry names and credentials immediately before an authorized release.
