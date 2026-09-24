# Server Memory Soak

Guards against the release-build allocator regression found on the demo
instance in September 2026: `v1.0.0-beta.1` grew ~20 MB/day while idle and
reached 755 MB after a month.

## Background

Release binaries are cross-compiled with `cargo zigbuild`. Zig 0.16 replaced
musl's `mallocng` in its bundled libc with Zig's `SmpAllocator`. The server
allocates query rows on sqlx-sqlite worker threads and frees them on tokio
workers; memory freed that way was never reused or returned, so RSS grew with
every database operation. `.github/workflows/release.yml` therefore pins
`ziglang==0.15.2`, whose libc still uses `mallocng`.

The growth only shows up in zigbuild outputs. `cargo build` on macOS or the
root `Dockerfile` (alpine `rust:1-alpine`) link a different allocator and stay
flat, so test the actual release toolchain.

## When to run

- Before bumping the pinned `ziglang` version or changing how Linux release
  binaries are built or linked.
- When a deployment's memory graph trends up without a matching rise in load.

## Steps

1. Build a Linux server binary with the release toolchain. On an arm64 Docker
   host, native aarch64 keeps the run fast:

   ```bash
   docker run --rm -v "$PWD":/src -w /src rust:1 bash -c '
     apt-get update -qq && apt-get install -y -qq python3-pip &&
     pip install --break-system-packages cargo-zigbuild ziglang==0.15.2 &&
     rustup target add aarch64-unknown-linux-musl &&
     cargo zigbuild --release --locked -p serverbee-server --target aarch64-unknown-linux-musl'
   ```

   `apps/web/dist` must exist first (see `tests/README.md`).

2. Run the soak (default 300s, fails above 4096 KB of RssAnon growth):

   ```bash
   scripts/memory-soak.sh target/aarch64-unknown-linux-musl/release/serverbee-server
   ```

## Expected results

- [ ] The script prints `PASS` and RssAnon stays within a few hundred KB after
      warmup while the check record count grows by thousands.
- [ ] Idle RssAnon after warmup is roughly 14 MB. A zig 0.16 build starts
      around 20 MB and grows ~2 MB/min under this load (`FAIL`).

## Checking a running instance

To tell allocator retention apart from page cache on a live Linux host, compare
`anon` and `file` in `/sys/fs/cgroup/memory.stat`, or `RssAnon` in
`/proc/<pid>/status`. On the affected demo instance, anon was 740 MB and file
only 31 MB.
