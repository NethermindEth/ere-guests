//! Stub crate for the Nethermind ZisK stateless-validator guest.
//!
//! The actual ELF is produced from C#/.NET source via the
//! `scripts/build-nethermind-guest.sh` script (invoked from `build.rs`). After
//! `cargo build` the ELF is available at the path stored in the compile-time
//! env var `NETHERMIND_GUEST_ELF`.

/// Absolute path to the built Nethermind ZisK guest ELF, set by `build.rs`.
pub const NETHERMIND_GUEST_ELF_PATH: &str = env!("NETHERMIND_GUEST_ELF");
