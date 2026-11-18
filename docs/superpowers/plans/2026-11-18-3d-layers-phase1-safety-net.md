# 3D Layers, Phase 1 (Safety Net) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A PSD/PSB 3D layer opens as a 3D layer showing Photoshop's stored render, can't be painted on by accident, can be rasterized, and saves back with its 3D data byte for byte.

**Architecture:** A new `LayerContent::Scene3d(Scene3dLayer)` variant in `photocraft-doc` holds the stored render (`cache`) and the layer's `PxSc` block (`psd_raw`). The scene itself isn't decoded in this phase. `photocraft-io` recognizes a `PxSc` block whose `pixelSourceType` is `3dPS`. The document-level `PxSD` block (textures) is already kept in `Metadata::psd_global_blocks` and stays there. The engine refuses pixel edits through the existing "not a pixel layer" checks and gains Layer › Rasterize › 3D.

**Tech Stack:** Rust 2024, `craft-psd` (descriptor and tagged-block types, `testgen`), the existing io/engine test harnesses.

**Spec:** `docs/superpowers/specs/2026-11-18-3d-layers-design.md` (phase 1 of 7).

## Global Constraints

- Non-test code never panics: no `unwrap`/`expect`/`panic!`/indexing on input-derived values (AGENTS.md *Never crash*).
- No `unsafe`. No `u8`-only pixel paths: test at 8 and 16 bits at least.
- Every action is an engine command; menu ids come from `crates/ui-egui/src/menu_catalog.rs`.
- New doc fields go through `crates/format/src/manifest.rs` and `convert.rs` with `#[serde(default)]`.
- L0–L6 still pass `cargo xtask wasm`; `cargo xtask layers` stays green.
- Private material is never committed: not to the repository, fixtures, logs, commit messages or PR text. Tests use synthetic files only.
- An untouched 3D file must save with its `PxSc` and `PxSD` bytes identical.
- Labels: "3D" (generic). Record it in `docs/feature-names.md`.
- Use `CARGO_TARGET_DIR=target/agent-3d` for every cargo command.

## Review Focus

1. **Moving or transforming a 3D layer** (Move tool, nudge, Free Transform): must either work on the stored pixels or return an error that names the 3D layer, never panic. Test in Task 3.
2. **Duplicating a 3D layer, then saving:** both copies stay 3D layers and reopen as 3D layers. Test in Task 2.
3. **A 3D layer inside a group, in a 16-bit document:** opens and round-trips the same as at top level, 8-bit. Test in Task 2.
4. **A malformed or truncated `PxSc` block:** opens as a pixel layer with the block kept and a warning, never a panic. Test in Task 2.
5. **Merge Down / Merge Visible with a 3D layer:** produces a pixel result from the stored render, and the merged layer carries no `PxSc`. Test in Task 3.

---

### Task 0: Unblock the workspace build

`cargo` 1.97 refuses to load the workspace: `crates/text/Cargo.toml` sets `default-features = false` on `craft = { workspace = true, … }`, which edition 2024 rejects because the root entry doesn't set it. Cargo ignored that flag in older versions, so removing it changes nothing about the build.

**Files:**
- Modify: `crates/text/Cargo.toml:21` and `:36`

- [ ] **Step 1:** Confirm the failure: `cargo metadata --format-version 1 >/dev/null`. Expected: error ending `default-features = false cannot override workspace's default-features`.
- [ ] **Step 2:** In both lines, change `craft = { workspace = true, default-features = false, features = [...] }` to `craft = { workspace = true, features = [...] }`. Keep the comment above line 21 accurate (it explains there is no `system` feature; that stays true).
- [ ] **Step 3:** `cargo metadata --format-version 1 >/dev/null` succeeds, and `CARGO_TARGET_DIR=target/agent-3d cargo check -p photocraft-text` passes.
- [ ] **Step 4:** Commit: `git commit -m "text: drop a default-features flag cargo rejects for inherited craft"`. This is its own commit, separate from the 3D work, so it can be sent upstream on its own.

### Task 1: The 3D layer kind in the document model and the native format

**Files:**
- Modify: `crates/doc/src/lib.rs` (`LayerContent` at ~606, `kind_name`, `Layer::surface` at ~753)
- Create: `crates/doc/src/scene3d.rs` (the struct and its docs), re-exported from `lib.rs`
- Modify: `crates/format/src/manifest.rs`, `crates/format/src/convert.rs` (`ContentM`)
- Modify: every other exhaustive `match` on `LayerContent` the compiler reports (compose, gpu, io, engine, ui-egui, automation, apps)
- Test: `crates/doc/src/scene3d.rs` (unit tests), `crates/format` existing round-trip test module

**Interfaces:**
- Produces:
  ```rust
  // photocraft_doc
  #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
  pub struct Scene3dLayer {
      /// Photoshop's (later: PhotoCraft's) last render of the scene, in document coordinates.
      pub cache: Surface,
      /// The layer's `PxSc` block data, written back verbatim while the scene is unchanged.
      pub psd_raw: Option<Arc<Vec<u8>>>,
  }
  pub enum LayerContent { …, Scene3d(Scene3dLayer) }
  ```
  - `LayerContent::Scene3d(_).kind_name() == "3D"`, article `"a"`.
  - `Layer::surface()` returns `Some(&s.cache)` for `Scene3d`. `Layer::surface_mut()` returns `None` (not paintable).
  - `ContentM::Scene3d { cache, psd_raw }` in `.pcraft`, using the same surface and `psd_raw` encodings as `ContentM::Shape`.

- [ ] **Step 1: Write the failing tests** in `crates/doc/src/scene3d.rs`:
  - `scene3d_is_a_3d_layer_with_read_only_pixels`: build `Layer::new("3D", LayerContent::Scene3d(Scene3dLayer { cache: Surface::new(PixelFormat::RGBA8), psd_raw: None }))`; assert `kind_name() == "3D"`, `surface().is_some()`, `surface_mut().is_none()`.
  - In format's round-trip tests, `scene3d_layer_round_trips_in_pcraft`: a document with one `Scene3d` layer (a 4 × 4 cache with a known pixel, `psd_raw = Some(Arc::new(vec![1, 2, 3]))`), saved and reopened; assert the layer is `Scene3d`, the pixel matches, and `psd_raw` equals `[1, 2, 3]`. Run it at RGBA8 and RGBA16.
- [ ] **Step 2:** Run `cargo test -p photocraft-doc scene3d` → fails to compile (no `Scene3dLayer`).
- [ ] **Step 3:** Add the struct and variant. Then `CARGO_TARGET_DIR=target/agent-3d cargo check --workspace --all-targets` and fix each exhaustive match it reports. The rule for each site: treat `Scene3d` like a `Smart` object whose cache is its pixels. Compose and gpu draw `cache`. Thumbnails and bounds use `cache`. Anything that edits pixels or content-specific parameters returns that code path's existing "not a pixel layer"/unsupported error. Don't add `_ =>` arms where the match was exhaustive before; name `Scene3d` explicitly.
- [ ] **Step 4:** `cargo test -p photocraft-doc -p photocraft-format` passes. `cargo xtask wasm` and `cargo xtask layers` pass.
- [ ] **Step 5:** Commit: `doc: add the 3D layer kind, stored render only`.

### Task 2: Open and save 3D layers in PSD and PSB

**Files:**
- Create: `crates/io/src/scene3d_map.rs` (detection), registered in `crates/io/src/lib.rs`
- Modify: `crates/io/src/psd_import.rs` (`layer_from_record`, ~507: a branch before `smart_key`; `rendered` set at ~646 includes `Scene3d`)
- Modify: `crates/io/src/psd_export.rs` (~556: `Scene3d` writes `cache` as the layer pixels; `PxSc` comes from `psd_raw`, replacing the `psd_blocks` entry like `Smart`)
- Test: `crates/io/src/scene3d_map/tests.rs` (or `#[cfg(test)] mod tests` in the file, matching nearby modules)

**Interfaces:**
- Consumes: `photocraft_doc::{Scene3dLayer, LayerContent::Scene3d}` from Task 1.
- Produces: `pub(crate) fn is_3d_pixel_source(pxsc: &[u8]) -> bool` in `scene3d_map.rs`. True when `VersionedDescriptor::parse_prefix(pxsc)` succeeds and its top-level `pixelSourceType` is `Value::Integer(0x3364_5053)` (the four bytes `3dPS`, observed in a Photoshop-written file). False otherwise, including for parse errors.

- [ ] **Step 1: Write the failing tests.** Fixture helper `fn pxsc(kind: i32) -> Vec<u8>`: `VersionedDescriptor::new(Descriptor::new("PixelSource").with("pixelSourceType", Value::Integer(kind)).with("descVersion", Value::Integer(2))).to_bytes()`. Fixture file: `craft_psd::testgen::layered(version, ColorMode::Rgb, depth, Compression::Rle)` with `TaggedBlock::new(*b"PxSc", pxsc(0x3364_5053))` pushed onto the first layer record's blocks, and a global `TaggedBlock::new(*b"PxSD", vec![0xA5; 64])`.
  - `pxsc_type_3dps_is_detected` / `other_pixel_source_types_are_not`: `is_3d_pixel_source` on `pxsc(0x3364_5053)` is true; on `pxsc(0x7669_6420)` and on `[0, 0, 0, 16, 0xFF]` it is false.
  - `three_d_layer_opens_as_scene3d` (PSD and PSB, 8 and 16 bit): `crate::import` gives a `Scene3d` layer whose cache equals the record's pixels, with no warnings mentioning 3D.
  - `untouched_three_d_file_saves_blocks_verbatim` (PSD and PSB): import → `document_to_psd` → bytes → `PsdFile::from_bytes`. The layer's `PxSc` data and the global `PxSD` data are byte-identical to the input. In PSB, `PxSD` is written with an `8B64` signature.
  - `duplicated_three_d_layer_stays_three_d`: duplicate the layer in the imported document, save, reopen: two `Scene3d` layers. (Review Focus 2)
  - `three_d_layer_in_a_group_round_trips`: the same layer nested in a group in a 16-bit file opens and saves the same. (Review Focus 3)
  - `malformed_pxsc_opens_as_pixels_with_warning`: `PxSc` data truncated to 7 bytes and an empty one: the layer opens as `Raster`, the block is still in `psd_blocks`, no panic. A 3dPS-typed block that fails to parse further down is not possible yet: detection reads only the top-level key. (Review Focus 4)
- [ ] **Step 2:** `cargo test -p photocraft-io --lib scene3d` → fails (no module).
- [ ] **Step 3:** Implement `is_3d_pixel_source`, the import branch (`LayerContent::Scene3d(Scene3dLayer { cache: self.record_surface(rec, &name), psd_raw: principal(b"PxSc") })`) and the export arm. A `PxSc` that is present but not a 3D source keeps today's behaviour (pixel layer, block preserved) without a warning. A `PxSc` that doesn't parse opens as pixels with the warning `layer "{name}": its 3D data could not be read; it opens as pixels and the data is kept as saved`.
- [ ] **Step 4:** `cargo test -p photocraft-io --lib` and `cargo xtask test-corpus --changed` pass (corpus floors don't drop).
- [ ] **Step 5:** Commit: `io: open and save PSD/PSB 3D layers with their data kept`.

### Task 3: Rasterize › 3D, and pixel edits refused

**Files:**
- Modify: `crates/engine/src/cmd/extra_cmds.rs` (`kind_of` ~740 adds `"3d"`; `rasterize_content` ~772 adds a `"3d"` arm; `rasterize_label` adds `Some("3d") => "Rasterize 3D"`; a new `spec!` for `layer.rasterize.3d` next to `layer.rasterize.type`)
- Modify: `crates/ui-egui/src/menu_catalog.rs:399` (insert `(&["Layer", "Rasterize"], "3D", None, "layer.rasterize.3d"),` after the Video row)
- Test: `crates/engine/src/cmd/extra_cmds/tests.rs`
- Regenerate: `docs/parity-checklist.md` (`cargo xtask parity`)

**Interfaces:**
- Consumes: `Scene3dLayer` (Task 1). Test documents are built directly in the engine (no PSD needed).
- Produces: command `layer.rasterize.3d`, params `{"layer":id?}` (no layer: every selected 3D layer), enabled when a 3D layer is targeted. `layer.rasterize.layer` and `layer.rasterize.allLayers` also rasterize 3D layers.

- [ ] **Step 1: Write the failing tests**, with a session holding one RGBA8 document and a `Scene3d` layer (cache with a known pixel, `psd_blocks` containing `(*b"PxSc", …)`):
  - `rasterize_3d_keeps_pixels_and_drops_pxsc`: `layer.rasterize.3d` → the layer is `Raster` with the same pixel, `psd_blocks` has no `PxSc`. Undo restores `Scene3d` with its `PxSc`.
  - `rasterize_layer_and_all_layers_cover_3d`: the same through `layer.rasterize.layer` and `layer.rasterize.allLayers`.
  - `rasterize_3d_on_a_pixel_layer_errs`: on a `Raster` layer, `Err` with "the layer is not a 3d layer".
  - `painting_on_a_3d_layer_is_refused`: `paint.stroke` and `edit.fill` on the 3D layer return `Err` whose text contains "3D layer, not a pixel layer". The document is unchanged.
  - `moving_a_3d_layer_never_panics`: `layer.translate` (`{"dx": 10, "dy": 5}`, what the Move tool dispatches) and `edit.transform.rotate180` on the 3D layer: `Ok` with the cache moved, or `Err` naming the 3D layer. Either way no panic, and the layer is still `Scene3d`. (Review Focus 1)
  - `merge_down_with_a_3d_layer_gives_pixels`: a `Scene3d` over a pixel layer, `layer.mergeDown` → one `Raster` layer whose pixel equals the composite and whose `psd_blocks` has no `PxSc`. (Review Focus 5)
- [ ] **Step 2:** `cargo test -p photocraft-engine --lib rasterize_3d` → fails (unknown command).
- [ ] **Step 3:** Implement. The `"3d"` arm takes `cache`, converts it to the document's pixel format if it differs (as the `"smart"` arm does) and removes `PxSc` from `psd_blocks`. The document's `PxSD` global block is left as is (harmless, and later phases will need it while any 3D layer remains). Where Step 1 shows the move or merge paths panicking or keeping `PxSc`, fix them at their source.
- [ ] **Step 4:** Run the gate:
  - `cargo test -p photocraft-engine --lib --tests`
  - `cargo test -p photocraft-engine --test panic_hunt -- --ignored`
  - `cargo test -p photocraft-ui-egui --lib`
  - `cargo xtask parity` (commit the regenerated `docs/parity-checklist.md`)
  - `cargo xtask panel-commands`
- [ ] **Step 5:** Commit: `engine: Layer > Rasterize > 3D; 3D layers refuse pixel edits`.

### Task 4: Docs, dev log and an optional maintainer check

**Files:**
- Modify: `docs/target-app-parity.md:352` (replace "3D is intentionally out of scope" with a dated line: 3D layers from older files are in scope for continuity; phase 1 opens and keeps them), add a 3D row or dimension, revision-history entry
- Modify: `docs/gaps.md` (a 3D entry: phases 2–7 open, link the spec), `ROADMAP.md` (one line)
- Modify: `docs/feature-names.md` ("3D" layer kind and "Rasterize › 3D": generic, recorded per `standards/naming.md`)
- Modify: `docs/file-format-parity.md` (PSD row: 3D layers open with stored render, `PxSc`/`PxSD` kept verbatim; edits not yet possible)
- Append: `log/devlog.md` (gitignored; local only)

- [ ] **Step 1: Maintainer-local check (optional, not reproducible from the repository).** Open a real Photoshop-authored 3D file, save it, and confirm the `PxSc`/`PxSD` bytes are unchanged.
- [ ] **Step 2:** Update the docs listed above. Dates are 2026-11-18 or the actual day of the change.
- [ ] **Step 3:** Run the final gate from AGENTS.md §5 for the touched crates (`cargo test -p photocraft-doc -p photocraft-format -p photocraft-io -p photocraft-engine`, `cargo clippy` on the same with `--all-targets -- -D warnings`, `cargo xtask layers`, `cargo xtask wasm`, `cargo xtask scorecard` if a checklist row moved). Paste the actual results into the PR description; list anything that couldn't run.
- [ ] **Step 4:** Commit: `docs: 3D layers are in scope for continuity; phase 1 recorded`.
