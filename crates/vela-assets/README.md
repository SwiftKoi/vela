# vela-assets

Asset import, transformation, and the content-addressed manifest.

**Owns:** Importer/Transformer registries, Manifest, Digests, font subsetting.

**Does not own:** Packing or shipping (vela-cli); runtime asset reads (vela-host).

Rank `2`. See `docs/ARCHITECTURE.md §1`.
