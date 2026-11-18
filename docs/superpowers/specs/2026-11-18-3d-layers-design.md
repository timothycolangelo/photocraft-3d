# 3D layers: open, edit and save Photoshop 3D PSDs

Status: draft for approval, 2026-11-18.

## Intent

Photoshop shipped a 3D workspace up to version 22.5 and then removed it. People still have PSDs
with 3D layers, and some depended on that workspace. PhotoCraft should open those files, show the
3D layers, let people **rotate, scale, retexture and relight** them, and save the result back to PSD.

The priority is continuity for existing files and workflows. Creating new 3D content (extrusions,
mesh presets, postcards, depth maps, DICOM, 3D printing, model import/export) comes after this,
in later projects.

## Goals

1. A PSD 3D layer opens as a **3D layer**, not as a flat pixel layer. It shows Photoshop's stored
   render until PhotoCraft has re-rendered it.
2. Its scene is decoded into PhotoCraft's own scene model: meshes, materials with their textures,
   lights, camera and object transform.
3. People can change the object's rotation, scale and position. They can replace or edit a
   material's textures and its basic properties (colours, shininess, opacity, bump/normal). They
   can add, remove and change lights: point, spot, infinite and image-based, with intensity,
   colour, direction and shadows.
4. PhotoCraft renders the scene itself: a CPU reference renderer, plus a wgpu renderer for
   interactive editing that is checked against it.
5. Saving a PSD writes freshly rendered pixels and a 3D block that includes the edits.
   Anything not decoded is written back byte for byte, and the user is warned once.
6. `.pcraft` stores the whole scene losslessly.
7. A 3D panel, a Properties view for the selected mesh, material or light, and the object/camera
   tools (orbit, roll, pan, slide, scale). These are laid out as a "3D" workspace preset.

## Non-goals (this project)

- Creating 3D content from 2D: extrusions, postcards, mesh presets, depth maps, volumes.
- Model file import/export (OBJ, DAE, STL, 3DS, KMZ, U3D, glTF) and 3D printing.
- Painting directly on the model surface. Retexturing in this project means replacing or editing
  the texture image (opened as a document, the way Photoshop opened textures).
- 3D animation timeline.
- A full ray tracer. The reference renderer is a shaded rasterizer with shadow maps. A
  progressive ray-traced final render is a later project.

## Anti-goals (a failure even if it "works")

- **Losing data from a file.** Opening and saving an untouched 3D PSD must give back an
  equivalent 3D layer: same scene bytes, same pixels. A save may never drop scene data it
  does not understand.
- **Crashing on a malformed scene.** The scene decoder treats every byte as hostile (AGENTS.md
  *Never crash*). A scene that can't be decoded falls back to today's behaviour (stored pixels,
  block kept verbatim) with a warning.
- **Copying Adobe.** No Adobe code, shaders, default materials, IBL images or strings. The
  layout of the block is learned from public specs and from files Photoshop wrote (clean-room).
- **Adobe feature names.** Labels follow `standards/naming.md` and are recorded in
  `docs/feature-names.md`. "3D", "Mesh", "Material", "Light" are generic. Product names
  ("Substance") are not used.
- **Breaking wasm or layering.** Everything at L0–L6 builds for `wasm32-unknown-unknown`.

## Constraints

- Rust only. Renderer on the existing wgpu stack. No webview, no JavaScript.
- Every action is an engine command with id, label, menu path, params, `enabled`, `run` and a
  graceful-failure test. `panic_hunt` stays green.
- No `u8`-only pixel paths. Textures and renders support 8/16/32f. Colour goes through `craft-cms`.
- New crates are registered in `xtask/src/layers.rs` with `[lints] workspace = true` and the
  `deny(clippy::unwrap_used, …)` root attribute.
- PSD parsing lives in `craft-psd` (storytold/craft-libs). If the 3D block decoder is
  format-generic, it goes to craft-libs and is pinned here. The PhotoCraft-specific mapping
  stays in `crates/io`.
- New doc fields go through `crates/format` (`manifest.rs`, `convert.rs`) with `#[serde(default)]`.
- `docs/target-app-parity.md` currently says 3D is out of scope. This project reverses that. The
  parity doc, `docs/gaps.md` and `ROADMAP.md` get a 3D dimension, with the date.
- **Samples.** Tests use small synthetic files written from the structure described below.
  Private material is never committed: not to this repository, the corpus, CI, logs or pull
  requests. A public sample can be added to storytold/photocraft-corpus later.
- **PSB as well as PSD.** Both containers are in scope; the 8-byte length rules for PSB apply to
  the global texture block.

## The format as observed in Photoshop-authored 3D files

Everything below is learned by reading files Photoshop wrote, using only the public PSD spec's
descriptor and tagged-block definitions (clean-room).

- **The 3D layer** is an ordinary layer record whose pixels are Photoshop's last render. It carries a
  layer block **`PxSc`** ("pixel source"), whose body is a version-16 **action descriptor**, the
  same encoding the spec documents for smart objects and effects. Its `pixelSourceType` is `3dPS`
  and it holds a `FrameReader` (type `A3FR`) with the scene.
- **The scene descriptor** has a version (major, minor and fix numbers), a bounding box,
  and lists: lights (`lite`: name, colour, hotspot, falloff, shadow, attenuation, type, on/off,
  4 × 4 matrix and position/angles), cameras (`caml`), meshes (`mshl`), materials (`mtll`),
  a scene tree (`key3DSceneTree`) and key-frames. Next to it are the 3D state (current camera and
  object position, per-axis object scale, field of view, orthographic flag, render settings,
  cross-section) and the source model's file name.
- **Meshes** store raw little-endian arrays in descriptor raw-data (`tdta`) items: positions
  (`verl`), normals (`nrml`), UVs (`uvl `) and a face/index stream (`faci`) that includes the
  material index per face group, plus a 4 × 4 matrix.
- **Materials** are a Phong-style model: ambient, diffuse, specular and emissive colours,
  shininess, roughness, opacity, reflection, refraction index, two-sided and wireframe flags, and a
  list of texture maps (`mapo`: map type, UV scale/offset, angle, wrap modes, filtering, and an
  index `Idnt` into the texture list, -1 when empty).
- **Textures** are not in the layer. A global block **`PxSD`** (8B64 in PSB) holds the texture images as
  **complete embedded PSD/PSB documents**, one per texture, in sequence. The image-based light's
  environment map, when there is one, is one of them, at 32 bits per channel. PhotoCraft's existing PSD reader can open
  each one, so a texture can be opened and edited as a normal document.
- The mesh index-stream layout, the framing between embedded documents and some trailing metadata
  in `PxSD` are only partly understood. Phase 3 finishes decoding them; anything still unknown stays
  byte for byte.
- **Versions.** The layout above is not confirmed against every Photoshop version up to 22.5.
  The decoder reads the scene version and treats versions it has not seen as best-effort, falling
  back to pixels plus verbatim blocks.

Implications for the design: decoding is reachable now. Most of it is the descriptor format
PhotoCraft already reads. A 3D document has a layer part (`PxSc`) and a document part (`PxSD`)
that must stay consistent; export rewrites both.

## Approaches considered

- **A. Full scene model with our own renderer (chosen).** Decode the block into a PhotoCraft
  scene, keep the original bytes alongside, render with a CPU reference plus wgpu, write edits back
  into the block on save. This is the most work. Edits survive a save, and it matches how the repo
  pairs `compose` (CPU oracle) with `gpu`.
- **B. Original scene kept as is, edits stored beside it.** Never rewrite the block. Store edits as
  PhotoCraft data and re-render the pixels. Less reverse-engineering, but older Photoshop sees only
  pixels, and a texture swap can't live outside the scene. Rejected.
- **C. GPU-only renderer.** Leaves CLI, tests and wasm-without-WebGPU with no reference renderer.
  Rejected.

Chosen in conversation: **A**.

## Design

### Crates

| Crate | Layer | Role |
|---|---|---|
| `photocraft-scene3d` | L1 (with `doc`) | Pure data: `Scene`, `Mesh` (positions, normals, UVs, indices, material slots), `Material`, `Texture` (a `Surface` ref plus wrap/scale/offset), `Light`, `Camera`, `Transform3` (f64 TRS). Bounds-checked constructors. No rendering. |
| `photocraft-render3d` | L2 | CPU reference renderer: tiled rasterizer with a depth buffer, perspective-correct UVs, Blinn-Phong-style shading of the material model, shadow maps, IBL from an equirectangular texture, ground-plane shadow catcher, antialiasing by supersampling. rayon per tile. Output is a `Surface` at the layer's depth. |
| `photocraft-gpu` (extended) | L3 | wgpu path for the same model. Tested against `render3d` within a tolerance, the same way `gpu` is tested against `compose`. |

`doc` gains `LayerContent::Scene3d(Scene3dLayer)`, where `Scene3dLayer { scene, cache: Option<Surface>,
psd_raw: Option<Arc<Vec<u8>>>, dirty: bool }`. `compose` draws a 3D layer from `cache`. The
engine re-renders the cache after any scene command.

**When re-rendering starts (builder's call, settled in review):** Photoshop's stored render
stays authoritative until the first scene edit. Opening a file, loading the GPU path or
selecting the layer never re-renders, so an untouched file looks and saves exactly as it was.
The first edit re-renders, sets `dirty`, and shows a one-time notice that PhotoCraft's render
may differ slightly from Photoshop's. Undoing back to the untouched state restores the
stored pixels and clears `dirty`. (Whether `scene3d` is its own crate or a
module of `doc` is left to the builder. Its own crate is preferred if `doc` would otherwise
depend on mesh math.)

### PSD mapping (`crates/io`, decoder possibly in craft-libs)

- **Import:** detect a layer with a `PxSc` block of type `3dPS` and the global `PxSD` block. Decode it into a `Scene` with a depth and size cap on
  every count and length. Keep the original bytes in `psd_raw`, and the stored pixels as `cache`.
  If decoding fails, keep today's pixel-layer behaviour, with a warning naming the layer.
- **Who reads edited files (decided 2026-11-18):** only PhotoCraft has to reopen an edited 3D
  layer correctly. Old Photoshop versions are not a target, and no testing against them is needed.
- **Export:** if the scene is unchanged, write `psd_raw` verbatim. If it changed, write a block
  that PhotoCraft reads back with every edit intact, including added or removed lights and
  replaced textures. It follows the original layout where that is known, rebuilds structure where
  an edit needs it, and keeps every undecoded part byte for byte. The new render is written as the
  layer pixels, so any reader still sees the edited look.
- Textures are the embedded documents in `PxSD`, opened with the existing PSD reader at their
  own depth and colour model. Retexturing replaces or edits one of them, and export re-embeds it.
  A document holds one `PxSD` for all its 3D layers, so the global block is kept on the document
  (next to `psd_global_blocks`) and the layer's maps refer to it by index.

### Commands (`crates/engine/src/cmd/scene3d_cmds.rs`)

Ids follow `menu_catalog.rs` if Photoshop's 3D menu entries are added there; otherwise a new
`3d.*` namespace. A first set:

- `3d.object.transform` (rotation as Euler degrees or quaternion, scale, translation; absolute or relative)
- `3d.camera.set` (position, target, fov or ortho, roll)
- `3d.material.set` (slot, properties)
- `3d.material.replaceTexture` (slot, channel, image path or layer/document reference)
- `3d.material.editTexture` (opens the texture as a document; saving it updates the scene)
- `3d.light.add` / `3d.light.remove` / `3d.light.set` (kind, colour, intensity, direction or position, cone, shadow, softness, IBL image)
- `3d.render` (re-render now; quality draft/final)
- `3d.layer.rasterize` (convert to a pixel layer)

Each one is undoable through `ops` history and fails with `Err` on bad params.

### UI (`crates/ui-egui`)

- A 3D panel: a tree with Environment, Scene, Camera, Meshes, Materials and Lights. Selecting an
  item shows its fields in Properties.
- Tools in the Move tool group while a 3D layer is active: orbit, roll, pan, slide and scale for
  the object or the camera, with an on-canvas axis widget. These are pure interaction; each drag
  dispatches `3d.object.transform` / `3d.camera.set`.
- A "3D" workspace preset that shows the 3D panel, Properties and Layers.
- Interactive drags render with the GPU path at draft quality. Releasing re-renders at final quality.
- Colours and radii from `theme::Tokens`. Checked with the offscreen `snapshot` example.

### Testing

- `scene3d`: constructor bounds, transform maths, serde round trip.
- `render3d`: synthetic scenes (unit cube, sphere, textured quad) with golden images at 8/16/32f;
  degenerate meshes, NaN vertices and huge counts give `Err` or an empty result, never a panic.
- `io`: round trip of untouched 3D PSDs from the corpus is byte-identical for the scene block. An
  edited scene re-opens with the edit. Fuzz the decoder.
- `gpu` vs `render3d` parity within a tolerance.
- Engine: every new command in `panic_hunt`, plus a graceful-failure test.
- Perf: re-render of a typical corpus scene at 24 MP in release, recorded in the dev log, with a
  budget added to `perf/budgets.toml`.

### Phases (each its own plan and pull request series)

1. **Safety net:** detect 3D layers, show them as 3D layers with stored pixels, keep the block,
   warn when pixels would be edited (a 3D layer cannot be painted on directly; rasterize first).
   Needs no decoding.
2. **Scene model and CPU renderer**, on synthetic scenes. `.pcraft` storage.
3. **PSD/PSB decode/encode** of `PxSc` and `PxSD`, tested on synthetic files built from the observed structure.
4. **Edit commands**: transform, camera, material, texture, light.
5. **wgpu renderer** for interaction.
6. **3D panel, tools, workspace preset.**
7. Docs: parity dimension, gaps, scorecard rows, feature names, dev log.

## Left to the builder

- Whether `scene3d` is a crate or a module of `doc`.
- The exact shading model, chosen to match observed Photoshop renders as closely as clean-room
  allows, within a stated tolerance.
- Whether the block decoder is generic enough for craft-libs (check craft-libs and its open PRs first).
- Internal data layouts, tile sizes, shadow-map resolution, supersampling factor.
