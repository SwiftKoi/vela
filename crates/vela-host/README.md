# vela-host

Adapter: the native window, the input table, and the idle tick they are polled on.

**Owns:** The window backend (`Window`, `Config`, `run`), the `App` trait its consumer implements, and the semantic actions input resolves to (`Action`, `Bindings`, `Key`).

**Does not own:** Anything a lower crate can do without a platform; the GPU (vela-render).

There are no `Host`, `Clock`, `Input`, or `Fs` traits, and this file used to claim there were. The clock
is the exception this crate is named for in `CONVENTIONS.md §2.2`, and it is `Instant` inside the window
loop rather than a trait: a second clock consumer is what would introduce one.

Rank `1`, **adapter**. See `docs/ARCHITECTURE.md §1` rules 1 and 2.
