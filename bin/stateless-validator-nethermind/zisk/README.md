# stateless-validator-nethermind-zisk

ZisK guest ELF for the **Nethermind** stateless-validator client.

Unlike `stateless-validator-ethrex` and `stateless-validator-reth`, Nethermind's
guest is **not** a Rust crate — it lives in the Nethermind repository as a
C#/.NET project (`src/Nethermind/Nethermind.Stateless.ZiskGuest`) and is
compiled by Nethermind's own `make build` toolchain (dotnet AOT → ZisK ELF).

This directory exists for layout consistency with the other guest clients.
The actual build is driven by [`build.rs`](build.rs), which:

1. Clones (or refreshes) `NethermindEth/nethermind` at branch
   `feature/benchmark` (override via `NETHERMIND_REF`, `NETHERMIND_REPO_URL`,
   `NETHERMIND_SRC`).
2. Runs `make -C src/Nethermind/Nethermind.Stateless.ZiskGuest build`.
3. Publishes `tools/StatelessInputGen` as a self-contained `linux-x64` binary
   used by host-side benchmarks.
4. Copies the resulting ELF into `OUT_DIR` and exposes its path via
   `NETHERMIND_GUEST_ELF` (re-exported through `NETHERMIND_GUEST_ELF_PATH`
   in `lib.rs`).

## Requirements

`git`, `docker` (with linux/amd64 support), `dotnet` (SDK ≥ 10.0), `make`.

## Build

```bash
cargo build \
    --manifest-path bin/stateless-validator-nethermind/zisk/Cargo.toml \
    --release
```

The ELF lands under
`bin/stateless-validator-nethermind/zisk/target/release/build/<hash>/out/stateless-validator-nethermind-zisk.elf`.

Set `SKIP_NETHERMIND_GUEST_BUILD=1` to skip the .NET build (useful for
`cargo check` / IDE indexing).
