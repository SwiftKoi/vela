# vela-host

Adapter: platform traits and their native implementations - window, input, filesystem, clock.

**Owns:** The Host/Clock/Input/Fs traits, the native backend, the save directory.

**Does not own:** Anything a lower crate can do without a platform; the GPU (vela-render).

Rank `1`, **adapter**. See `docs/ARCHITECTURE.md §1` rules 1 and 2.
