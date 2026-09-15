# Fonts

Shipped faces. The engine reads these at runtime; `BUILD_AND_ASSETS.md §2` turns them into
subsets per locale at build time, which is why nothing here is optimised by hand.

## LiberationSans-Regular.ttf

- **Upstream:** Liberation Fonts 2.x, from `arch` package `ttf-liberation`
  (`/usr/share/fonts/liberation/LiberationSans-Regular.ttf`), taken 2026-09-14.
- **License:** SIL Open Font License 1.1 — see `LICENSE.LiberationSans.txt`, copied
  verbatim from `/usr/share/licenses/ttf-liberation/LICENSE`.
- **Why this one:** it is metric-compatible with Arial, has full Latin coverage, and is
  small enough to vendor. The engine needs *a* real face, not a special one — a layout golden
  is only meaningful against a font that is actually checksummed into the repository, since
  a fixture read from a system path is a fixture that differs per machine and the golden
  stops meaning anything.
