# vela-plugin

The WASM plugin host: the capability ABI and its versioned surface.

**Owns:** PluginHost, capability grants, ABI version negotiation.

**Does not own:** The registries plugins write into; those live in the owning crates.

Rank `8`. See `docs/ARCHITECTURE.md §1`.
