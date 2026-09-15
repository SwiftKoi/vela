//! The protocol, and the capabilities it advertises.
//!
//! Nothing here yet: `M10-tooling.md` item 2 is the transport — `initialize`, document sync, and
//! publishing the diagnostics [`crate::diagnostics`] already knows how to produce. What exists today
//! is the analysis the server will serve, and the parity test that keeps it equal to `vela check`.
//!
//! The shape it will take: a loop over framed JSON-RPC messages on stdin/stdout, holding one
//! [`vela_compile::Session`] per workspace, and mapping byte offsets to LSP positions. The mapping is
//! worth writing before the loop, because the LSP counts *UTF-16 code units* in a line while every
//! span in this compiler counts bytes.

/// The language server.
pub struct Server;
