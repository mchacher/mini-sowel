# Architecture: Spec 056

- `firmware/sdkconfig.defaults`: `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=512`.
- `firmware/src/`: the buffers that must stay internal allocated with `heap_caps_malloc(...,
  MALLOC_CAP_INTERNAL)` or an existing internal path; the install's step lines through the job
  that runs it (`http_server.rs`, `install_ops.rs`), reading `heap_caps_get_free_size` and
  `heap_caps_get_minimum_free_size` for `MALLOC_CAP_INTERNAL`.
- `crates/core/src/recipes/install/catalogue.rs`: `serde` structs in place of `Value`, kept once
  per read in the install state (`install/mod.rs`).
