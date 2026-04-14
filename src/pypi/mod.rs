// TODO(mcg): Wheel caching and installation improvements
//
// The current implementation caches downloaded .whl blobs by SHA256 and
// re-extracts into site-packages on every install. Four improvements are
// planned, informed by the designs of prefix-dev/rip and njsmith/posy:
//
// 1. HTTP-level caching — use the `http-cache-semantics` crate to store
//    responses in a content-addressed file store with proper Cache-Control
//    and ETag support. Avoids redundant downloads even when the hash is
//    not known upfront. (rip and posy both do this.)
//
// 2. Metadata caching — cache parsed METADATA files keyed by artifact hash,
//    so dependency resolution can read metadata without re-downloading the
//    full wheel. (rip does this in a separate `metadata/` cache tier.)
//
// 3. Lazy range-request metadata reading — use HTTP Range requests to read
//    the zip central directory and METADATA from the end of a remote wheel
//    without downloading the entire file. PyPI supports this. (posy does
//    this via a `LazyRemoteFile` implementing Read + Seek over HTTP.)
//
// 4. Content-addressed extraction store with hard links — extract each wheel
//    once into a shared store (like conda's `pkgs/` directory), then
//    hard-link files into the target site-packages. Makes repeated
//    `ana prepare` near-instant for previously-seen wheels. This mirrors
//    conda's approach and deliberately stops short of runtime sys.path
//    manipulation (as posy does).

pub mod installer;
pub mod wheel;
