# 3D Layers, Phase 2 (Scene Model and CPU Renderer) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PhotoCraft has its own 3D scene model and a CPU reference renderer that draws it into a layer-sized `Surface` at any depth and colour model, and a 3D layer's scene is saved losslessly in `.pcraft`.

**Architecture:** A new L1 crate `photocraft-scene3d` holds the scene as pure, bounds-checked data (meshes, materials, textures, lights, cameras, object transform). A new L2 crate `photocraft-render3d` renders a `Scene` with a tiled, supersampled triangle rasterizer: Blinn-Phong shading, texture maps sampled through `craft-texture`, shadow maps, image-based light and a ground-plane shadow catcher, in scene-linear light, with colour conversion through `craft-cms`. `Scene3dLayer` gains an optional scene and a `dirty` flag, and `photocraft-format` stores them. Nothing reads a scene out of a PSD yet (phase 3) and no command edits one yet (phase 4), so everything here is exercised on synthetic scenes.

**Tech Stack:** Rust 2024, rayon, `craft-cms` (colour transforms), `craft-texture` (float image planes and texture sampling, from storytold/craft-libs at the workspace's pinned revision), the existing `photocraft-raster` `Surface`.

**Spec:** `docs/superpowers/specs/2026-11-18-3d-layers-design.md` (phase 2 of 7). Phase 1 plan: `docs/superpowers/plans/2026-11-18-3d-layers-phase1-safety-net.md`.

## Global Constraints

- Non-test code never panics: no `unwrap`/`expect`/`panic!`/`unreachable!`, no slice indexing on input-derived values, `checked_*`/`saturating_*` for counts and sizes, NaN/inf guarded before any cast (AGENTS.md *Never crash*). Both new crate roots carry `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]` and their `Cargo.toml` has `[lints] workspace = true`.
- No `unsafe`.
- No `u8`-only pixel paths: the renderer writes RGBA8, RGBA16 and RGBA32F targets, and textures may be any `PixelFormat`. Tests run at 8, 16 and 32f.
- Never assume sRGB. Shading happens in scene-linear RGB (Rec. 709 primaries, D65, 1.0 = diffuse white, the `craft-render-parity` convention). Every conversion into or out of it goes through a `craft_cms::transform::cached` transform built from a profile the caller passes. The renderer never picks a document profile itself.
- Layering: `scene3d` is L1 and `doc` depends on it (an intra-layer chain `["scene3d", "doc"]` in `xtask/src/layers.rs`); `render3d` is L2. Neither uses egui, eframe, winit or rfd. Both build for `wasm32-unknown-unknown` (`cargo xtask wasm`; its five pre-existing errors in `crates/engine/src/prefs.rs` are not this plan's, and no new error may appear).
- Every count and size read from a scene is capped by a `Limits` value before anything is allocated. Defaults: 20,000,000 vertices and 20,000,000 triangles per scene (the `craft-mesh` values), 4,096 meshes, 4,096 materials, 256 textures, texture side 16,384, 64 lights, 16 cameras, output 268,435,456 samples (2^28) after supersampling, 4,194,304 triangle-bin entries per tile.
- New doc fields go through `crates/format/src/manifest.rs` and `convert.rs` with `#[serde(default)]`, so `.pcraft` files from phase 1 still load.
- Clean-room: no Adobe code, shaders, default materials, environment images or strings. Shading formulas come from public literature (Blinn 1977; Williams 1978 shadow maps; equirectangular mapping), cited in doc comments.
- Labels: "3D", "Mesh", "Material", "Light", "Camera" (generic). No new user-visible label is added in this phase.
- Private material is never committed: not to the repository, fixtures, logs, commit messages or PR text. Tests use synthetic scenes only.
- House rule 15: commit messages and PR text carry no agent attribution (no `Co-Authored-By` or session trailers).
- Use `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/agent-3d` for every cargo command. Check `df -h /` before a large build; under 4 GB free, delete large linked executables in `target/agent-3d/debug/deps` (never `.rlib`/`.rmeta`) and `target/agent-3d/debug/examples`.

## Conventions every task uses

- **Space:** right-handed, +Y up, the camera looks down its local −Z (glTF 2.0 convention). Object transform is translation · rotation · scale applied to column vectors. Angles in the public API are radians in f64.
- **UVs:** origin at the top left, v pointing down (the `craft-texture` and glTF convention).
- **Output:** a `Surface` in the requested `PixelFormat` covering `RenderTarget::rect` in document pixels, straight (not premultiplied) alpha, alpha 0 where nothing is drawn. A target format without alpha is refused with `RenderError::Unsupported`.
- **Precision:** transforms, camera and projection in f64; vertex arrays stored as f32; per-sample shading in f32.

## Rulings in this plan

- `Scene3dLayer::cache` stays a `Surface`, not the spec's `Option<Surface>`: every 3D layer has a render (Photoshop's or ours), and phase 1 code relies on it. Cost if wrong: a later change to `Option`.
- `scene3d` types carry no serde. `photocraft-format` mirrors them in `SceneM`, as it does for text and shapes, because mesh arrays must be stored as blobs, not JSON numbers. This replaces the spec's "serde round trip" test in `scene3d` with the `.pcraft` round trip in Task 6.
- Textures live in the `Scene` for now. Whether decoded textures move to the document so several 3D layers share one `PxSD` is decided in phase 3.
- `Material::shininess` is the Blinn-Phong exponent. Mapping Photoshop's stored values onto it is phase 3's job.
- Tests check computed pixel values, not golden image files, so no binary fixtures enter the repository.
- craft-libs has no renderer; `craft-mesh-ops` (open PR storytold/craft-libs#1233: normals, tangents, BVH, primitives) is not merged at the workspace's pinned revision, so this plan does not depend on it. When it lands, smooth normals and tangents can move to it.

## Review Focus

1. **A hostile scene** (index past the vertex count, NaN or infinite positions, a triangle count over the limit, a face group past the index stream, a material slot that does not exist, a zero-size texture) must give an `Err` from the constructor or the renderer, never a panic or a huge allocation. Tests in Tasks 1 and 2, and for `.pcraft` input in Task 6.
2. **A camera inside or behind the geometry, and triangles crossing the near plane:** these must be clipped in homogeneous space and render the visible part, never garbage spans, division by zero or a stalled loop. Test in Task 2.
3. **Degenerate input:** zero-area triangles, zero-length normals, coincident vertices, a camera with zero field of view or zero-size viewport, a light at the shaded point. Each gives a finite result or an `Err`, never NaN pixels. Tests in Tasks 2 and 3.
4. **Large output:** a 24 MP target at final quality must stay within a bounded working set (tiles, not a whole-image supersampled buffer). Test in Task 2 (memory bound by construction plus a cap test) and a timing in Task 7.
5. **Non-RGB and deep targets:** rendering into a Grayscale or CMYK profile, and into 16-bit and 32f surfaces, gives correct values through `craft-cms` (a mid-grey diffuse quad under a known light gives the expected value in each). Test in Task 2.

---

### Task 1: `photocraft-scene3d`, the scene as data

**Files:**
- Create: `crates/scene3d/Cargo.toml`, `crates/scene3d/src/lib.rs`, `crates/scene3d/src/math.rs`, `crates/scene3d/src/mesh.rs`, `crates/scene3d/src/material.rs`, `crates/scene3d/src/light.rs`, `crates/scene3d/src/camera.rs`, `crates/scene3d/src/tests.rs`
- Modify: root `Cargo.toml` (`photocraft-scene3d = { path = "crates/scene3d" }` in `[workspace.dependencies]`), `xtask/src/layers.rs` (`("scene3d", Class::Layer(1))` and the chain `&["scene3d", "doc"]` in `INTRA_LAYER_ORDER`, with a comment saying why)

**Interfaces:**
- Produces (all `#[derive(Clone, Debug, PartialEq)]`, no serde):
  - `math`: `Vec3 { x, y, z: f64 }`, `Quat { x, y, z, w: f64 }` (unit; `Quat::from_euler_xyz(rx, ry, rz)`, `Quat::rotate(Vec3)`), `Mat4([[f64; 4]; 4])` column-major with `mul`, `mul_point`, `mul_dir`, `inverse() -> Option<Mat4>`, `Transform3 { translation: Vec3, rotation: Quat, scale: Vec3 }` with `matrix() -> Mat4` (T·R·S) and `Transform3::IDENTITY`.
  - `Mesh::new(name: String, positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, indices: Vec<u32>, groups: Vec<FaceGroup>, transform: Transform3, limits: &Limits) -> Result<Mesh, SceneError>`. `FaceGroup { first_triangle: u32, triangle_count: u32, material: u32 }`. `normals` and `uvs` are empty or the same length as `positions`. Accessors only; fields private.
  - `Material { name, ambient, diffuse, specular, emissive: [f32; 3] (scene-linear RGB), shininess: f32 (Blinn-Phong exponent, ≥ 0), opacity: f32 (0..=1), reflection: f32 (0..=1), two_sided: bool, maps: Vec<TextureMap> }`. `TextureMap { channel: MapChannel, texture: u32, uv_scale: [f32; 2], uv_offset: [f32; 2], angle: f32, wrap: [Wrap; 2], strength: f32 }`; `MapChannel { Diffuse, Specular, Shininess, Opacity, Emissive, Bump, Normal, Reflection, Environment }`; `Wrap { Repeat, Clamp }`.
  - `Texture { name: String, surface: Surface, icc: Option<Arc<Vec<u8>>> }` (the image in its own format and profile; `None` means "the caller's working profile for that mode").
  - `Light { name, kind: LightKind, color: [f32; 3], intensity: f32, on: bool, casts_shadow: bool, shadow_softness: f32 }`; `LightKind { Point { position: Vec3, attenuation: Attenuation }, Spot { position: Vec3, direction: Vec3, hotspot: f64, falloff: f64, attenuation: Attenuation }, Infinite { direction: Vec3 }, ImageBased { texture: u32, rotation: Quat } }`; `Attenuation { None, Linear { inner: f64, outer: f64 } }` (cone angles in radians).
  - `Camera { name, transform: Transform3, projection: Projection, near: f64, far: f64 }`; `Projection { Perspective { fov_y: f64 }, Orthographic { height: f64 } }`.
  - `GroundPlane { enabled: bool, height: Option<f64> (None = lowest point of the scene's bounds), shadow_opacity: f32 }`.
  - `Scene::new(SceneParts { meshes, materials, textures, lights, cameras, active_camera: u32, object: Transform3, ground: GroundPlane }, limits: &Limits) -> Result<Scene, SceneError>`, accessors for each part, `bounds() -> Option<(Vec3, Vec3)>` (world space, finite positions only).
  - `Limits` with the Global Constraints defaults (`Limits::default()`); `SceneError { Invalid(String), TooLarge(String) }` implementing `Display` and `std::error::Error`.
- Consumes: `photocraft_raster::Surface`, `photocraft_color::PixelFormat`.

`Mesh::new` and `Scene::new` are the only ways to build these types, and they check everything a renderer relies on: finite positions, normals and UVs; every index below the vertex count; `indices.len()` a multiple of 3; every face group inside the triangle range (checked in u64) and naming an existing material; every `TextureMap::texture` and `ImageBased::texture` naming an existing texture; `active_camera` in range; camera `near > 0`, `far > near`, `fov_y` in `(0, π)`, ortho `height > 0`; spot `0 ≤ hotspot ≤ falloff < π`; every count against `Limits` before allocating; non-finite material values refused; `opacity` and `reflection` outside 0..=1 refused (not clamped).

- [ ] **Step 1: Write the failing tests** in `crates/scene3d/src/tests.rs`: `quat_rotates_x_to_y_about_z` (90° about Z maps (1,0,0) to (0,1,0) within 1e-12), `transform_matrix_is_t_r_s` (point (1,0,0), scale 2, rotate 90° about Z, translate (0,0,5) → (0,2,5)), `inverse_of_singular_is_none`, `mesh_rejects_index_past_vertices`, `mesh_rejects_nan_position`, `mesh_rejects_group_past_triangles` (first_triangle = u32::MAX, count 2), `mesh_rejects_partial_triangle`, `mesh_rejects_mismatched_normals`, `scene_rejects_missing_material`, `scene_rejects_missing_texture`, `scene_rejects_active_camera_out_of_range`, `scene_rejects_over_limit_triangles` (a `Limits` with `max_triangles: 1` and two triangles, so nothing large is allocated), `camera_rejects_zero_fov_and_bad_near_far`, `spot_rejects_hotspot_wider_than_falloff`, `bounds_of_unit_cube_under_object_scale_2` ((-1,-1,-1)..(1,1,1) for a ±0.5 cube), `unit_cube_and_quad_helpers_build` (the test helpers `unit_cube()` and `quad()` that later tasks use live in `scene3d` behind `#[cfg(any(test, feature = "testgen"))]` as `pub mod testgen`).
- [ ] **Step 2:** Run `cargo test -p photocraft-scene3d` and see them fail (crate missing, then each assertion).
- [ ] **Step 3:** Implement the crate. Root attribute per Global Constraints. `testgen` exposes `unit_cube(material: u32) -> Mesh` (±0.5, 24 vertices with face normals and per-face UVs 0..1), `quad(size: f32, material: u32) -> Mesh` (in the XY plane facing +Z, UVs top-left origin), `grey_material(v: f32) -> Material`, `camera_looking_at(eye: Vec3, target: Vec3, fov_y: f64) -> Camera`, and `scene_of(meshes, materials, lights, camera) -> Scene`.
- [ ] **Step 4:** Run `cargo test -p photocraft-scene3d`, `cargo clippy -p photocraft-scene3d --all-targets -- -D warnings`, `cargo xtask layers`. Expected: pass, clean, OK.
- [ ] **Step 5:** Commit: `scene3d: the 3D scene as bounds-checked data`.

### Task 2: `photocraft-render3d`, the rasterizer and colour pipeline

**Files:**
- Create: `crates/render3d/Cargo.toml`, `crates/render3d/src/lib.rs`, `src/target.rs` (target, settings, colour pipe), `src/clip.rs`, `src/raster.rs` (tiles, bins, depth), `src/shade.rs` (only the unlit/emissive and ambient terms in this task), `src/tests.rs`
- Modify: root `Cargo.toml` (`photocraft-render3d = { path = "crates/render3d" }`; `craft-texture` at the same `git`/`rev` as the other craft-libs crates, licence `MIT OR Apache-2.0`, recorded in the PR), `xtask/src/layers.rs` (`("render3d", Class::Layer(2))`)

**Interfaces:**
- Consumes: Task 1's `Scene` and accessors.
- Produces:
  - `RenderTarget<'a> { rect: Rect, format: PixelFormat, profile: &'a craft_cms::Profile }`: `rect` is the document area the camera's viewport covers (its aspect ratio is the viewport's); `profile` is the profile of the target's colour mode (RGB, Grayscale or CMYK).
  - `TextureProfiles<'a> { rgb: &'a Profile, gray: &'a Profile, cmyk: &'a Profile }`: what a `Texture` with `icc: None` is assumed to be in, by its mode.
  - `RenderSettings { quality: Quality, limits: Limits }`; `Quality { Draft, Final }`. Draft: 1 sample per pixel, shadow maps 512. Final: 4 × 4 ordered-grid supersampling, shadow maps 2,048.
  - `render(scene: &Scene, target: &RenderTarget, textures: &TextureProfiles, settings: &RenderSettings, interrupt: Interrupt) -> Result<Surface, RenderError>` (`photocraft_raster::interrupt::Interrupt`; pass `Interrupt::NONE` when not cancellable); `RenderError { Scene(SceneError), Unsupported(String), TooLarge(String), Color(String), Interrupted }`.
  - `render` checks `interrupt.cancelled()` once per tile and returns `Interrupted`, and reports `interrupt.progress` as tiles finish.

How it works (the choices the tests don't pin):
- Vertices go to clip space with f64 `projection · view · object · mesh` matrices. Each triangle is clipped against the near plane in homogeneous space (Sutherland–Hodgman on `z ≥ −w` for the right-handed −Z-forward projection), giving 0, 1 or 2 triangles; triangles entirely outside any other frustum plane are dropped; the guard band handles the rest. Back faces are culled unless the triangle's material is `two_sided`.
- The output rect is split into 64 × 64 pixel tiles. A binning pass records, per tile, the triangles whose screen bounding box touches it (entries counted against `max_bin_entries` before reserving). Tiles render in parallel with rayon; each owns its supersampled colour and depth buffers (at most 64·64·16 samples), so memory does not grow with the image.
- Coverage uses edge functions with the top-left fill rule; attributes are interpolated perspective-correctly (barycentrics divided by w). Depth test is less-than on interpolated z/w.
- The resolved tile (box filter over its samples, alpha = coverage × surface opacity) is converted with one `transform::cached(LinearSrgb, target.profile)` transform (`TransformOptions::default()`, relative colorimetric) and written with `Surface::try_write_region`. A Grayscale or CMYK target takes the transform's channel count; alpha is carried, not converted.
- Before any allocation, `render` checks `rect` area × samples per pixel against `max_output_samples` and the scene against its limits.

- [ ] **Step 1: Write the failing tests** in `crates/render3d/src/tests.rs`, all with an emissive-only material so lighting doesn't enter yet:
  - `emissive_quad_fills_its_projection`: a 2 × 2 quad at z = 0, camera at (0,0,2) looking at the origin with `fov_y` = 2·atan(0.5) → the quad covers exactly the middle half of a 64 × 64 target; pixel (32,32) has the emissive colour and alpha 1, pixel (2,2) alpha 0. At 8, 16 and 32f.
  - `nearer_triangle_wins_depth`: two overlapping emissive quads, red at z = 0.5, blue at z = 0; the centre is red at any submission order.
  - `back_faces_are_culled_unless_two_sided`.
  - `near_plane_clipping_keeps_the_visible_part`: a quad from z = −1 to z = +3 with the camera at z = 2; the render is finite, the near part is drawn and nothing outside the quad's projection is touched.
  - `camera_inside_a_cube_sees_its_inside_faces` (two-sided material).
  - `degenerate_triangles_draw_nothing`: zero-area and coincident-vertex triangles give alpha 0 everywhere and no NaN.
  - `gray_and_cmyk_targets_convert_through_cms`: a mid-grey emissive (0.18 linear) quad into a Grayscale target with `Builtin::SGray` and into a CMYK target with `Builtin::CoatedFogra39`; the centre equals `transform::cached(LinearSrgb, profile).eval([0.18; 3])` within 1e-4.
  - `target_without_alpha_is_unsupported`.
  - `oversized_target_is_refused_before_allocating` (a `Limits` with `max_output_samples: 1_000`, a 100 × 100 Final target → `TooLarge`).
  - `twenty_four_mp_final_uses_tiles`: checks `tile_buffer_samples(Quality::Final)` equals 64·64·16 (the per-tile buffer size is a function, not a whole-image buffer).
  - `supersampling_antialiases_an_edge`: a quad edge across a pixel centre gives alpha strictly between 0 and 1 in Final and 0 or 1 in Draft.
- [ ] **Step 2:** Run `cargo test -p photocraft-render3d` and see them fail.
- [ ] **Step 3:** Implement as above.
- [ ] **Step 4:** Run `cargo test -p photocraft-render3d`, clippy `-D warnings`, `cargo xtask layers`, `cargo xtask wasm` (only the five pre-existing `prefs.rs` errors). Record timings of the test suite in the report.
- [ ] **Step 5:** Commit: `render3d: tiled, supersampled rasterizer with colour through craft-cms`.

### Task 3: Lighting

**Files:**
- Modify: `crates/render3d/src/shade.rs`, `src/tests.rs`

**Interfaces:**
- Consumes: Task 1 `Light`, `Material`; Task 2 `render`.
- Produces: `shade.rs` `fn shade(material: &Material, frag: &Fragment, lights: &[LightView], view_dir: [f32; 3]) -> [f32; 4]` (crate-private; `Fragment` holds the interpolated position, normal and UV of a sample, `LightView` a light in world space with its shadow lookup, both defined in this task), used again by Tasks 4 and 5.

The shading model (Blinn 1977), all in scene-linear RGB, per light `L` that is `on`:
`colour = emissive + ambient · Σ(ambient-only lights, none in this phase, so 0) + Σ_L intensity_L · color_L · att_L · (diffuse · max(n·l, 0) + specular · max(n·h, 0)^shininess · [n·l > 0])`, with `h = normalize(l + v)`. Point and spot lights use inverse-square fall-off scaled so intensity 1 gives full colour at distance 1, times `Attenuation::Linear` (1 inside `inner`, linear to 0 at `outer`, measured from the light); spots multiply by a smoothstep from `falloff` to `hotspot` on the angle to `direction`. Infinite lights have no fall-off. `ImageBased` lights are ignored until Task 5. Alpha = `opacity`. A two-sided material flips `n` toward the viewer. A zero-length interpolated normal falls back to the geometric face normal; a light at the shaded point contributes 0.

- [ ] **Step 1: Write the failing tests** (each at 8, 16 and 32f where it reads pixels): `infinite_light_head_on_gives_diffuse` (grey 0.5 diffuse quad facing +Z, infinite light along −Z, intensity 1 → centre equals the target-profile conversion of linear 0.5), `light_at_60_degrees_halves_diffuse`, `light_behind_the_surface_gives_black_and_no_specular`, `point_light_inverse_square` (distance 2 gives a quarter), `spot_outside_falloff_is_dark_inside_hotspot_is_full`, `specular_peak_at_mirror_angle`, `off_lights_contribute_nothing`, `zero_length_normal_uses_face_normal`, `light_at_surface_point_is_finite`, `opacity_sets_alpha`.
- [ ] **Step 2:** Run and see them fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p photocraft-render3d`, clippy.
- [ ] **Step 5:** Commit: `render3d: Blinn-Phong lighting for point, spot and infinite lights`.

### Task 4: Texture maps

**Files:**
- Create: `crates/render3d/src/texture.rs`
- Modify: `src/shade.rs`, `src/tests.rs`

**Interfaces:**
- Consumes: Task 1 `Texture`, `TextureMap`; Task 2 `TextureProfiles`; Task 3 `shade`.
- Produces: `texture.rs` `struct TextureSet` built once per `render` call: each scene texture converted to a `craft_texture::Image` (RGBA, scene-linear) through `craft-cms` from its `icc` profile (or the matching `TextureProfiles` entry), sized against `Limits` with `craft_texture::Limits`; Bump and Normal maps and Opacity/Shininess/Reflection are read as data (no colour conversion: the raw channel values, 0..1).

Sampling uses `Image::sample_axes(u', v', [wrap_u == Repeat, wrap_v == Repeat])` (bilinear), with `(u', v') = rotate(angle) · (uv · uv_scale) + uv_offset`. Diffuse, Specular and Emissive maps multiply the material colour; Opacity multiplies `opacity`; Shininess multiplies `shininess`; Normal maps are tangent-space (RGB → [−1, 1]); Bump maps perturb the normal by the height gradient sampled at ±1 texel, scaled by `strength`. Tangents come from per-triangle UV derivatives (a degenerate UV mapping falls back to no perturbation). A mesh without UVs ignores its maps.

- [ ] **Step 1: Write the failing tests**: `checker_diffuse_map_lands_on_quad_corners` (2 × 2 checker texture, black top-left; the quad's top-left render quadrant is black, top-right white, emissive-free lit head-on), `repeat_vs_clamp_wrap` (uv_scale 2: Repeat shows the checker twice, Clamp repeats the edge), `uv_offset_and_angle_move_the_pattern`, `opacity_map_cuts_alpha`, `normal_map_tilts_lighting` (a constant normal-map texel pointing 60° away halves diffuse), `bump_flat_is_unchanged`, `sixteen_bit_and_float_textures_sample_the_same` (the same checker as RGBA8, RGBA16, RGBA32F gives the same render within 1/255), `texture_in_another_profile_is_converted` (a texture with `icc` set to `Builtin::DisplayP3`'s bytes renders as its converted colour), `oversized_texture_is_refused` (`Limits` side 4, an 8 × 8 texture → `TooLarge`), `mesh_without_uvs_ignores_maps`.
- [ ] **Step 2:** Run and see them fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p photocraft-render3d`, clippy, `cargo xtask wasm`.
- [ ] **Step 5:** Commit: `render3d: texture maps sampled through craft-texture`.

### Task 5: Shadows, image-based light and the ground plane

**Files:**
- Create: `crates/render3d/src/shadow.rs`, `crates/render3d/src/ibl.rs`
- Modify: `src/shade.rs`, `src/raster.rs` (ground plane), `src/tests.rs`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces: `shadow.rs` `ShadowMap` per shadow-casting light (Williams 1978): infinite → one orthographic map fitted to the scene bounds; spot → one perspective map with the falloff cone; point → six 90° faces. Built in parallel before the tiles render; resolution from `Quality`; depth bias of 2 texels along the normal; percentage-closer filtering over a kernel whose radius grows with `shadow_softness` (0 = hard, 1 = 8 texels). `ibl.rs` `Environment` from an `ImageBased` light's texture (equirectangular, longitude 0 at −Z, rotated by its `rotation`): a cosine-convolved 64 × 32 irradiance map for diffuse, and a reflection lookup of the original map for the `reflection` term; at most one active `ImageBased` light (more → `Unsupported`).

Ground plane: when `GroundPlane::enabled`, an infinite horizontal plane at `height` (or the lowest bound) is rasterized with depth but no colour. Where it is in shadow, the output gets black with alpha `shadow_opacity × shadow amount`; elsewhere it stays transparent. It never hides the object.

- [ ] **Step 1: Write the failing tests**: `cube_casts_a_shadow_on_a_quad_below` (infinite light straight down, a quad under a cube: the point under the cube is darker than a point outside it, both finite), `shadow_free_light_casts_none`, `spot_and_point_shadows_cover_their_side`, `soft_shadow_has_a_gradient_hard_has_an_edge`, `ground_plane_shadow_only_where_shadowed` (alpha 0 away from the shadow, alpha ≈ shadow_opacity under the cube, at 8 and 16 bits), `uniform_environment_lights_diffuse_evenly` (a constant-colour 8 × 4 equirect at linear 1.0 gives a diffuse 0.5 quad facing any direction its value 0.5 within 2 %), `environment_rotation_moves_its_bright_spot`, `two_image_based_lights_are_unsupported`, `shadow_maps_respect_quality` (Draft builds 512² maps, Final 2048²).
- [ ] **Step 2:** Run and see them fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p photocraft-render3d`, clippy, `cargo xtask wasm`.
- [ ] **Step 5:** Commit: `render3d: shadow maps, image-based light and ground-plane shadows`.

### Task 6: The scene in the document and in `.pcraft`

**Files:**
- Modify: `crates/doc/Cargo.toml` (`photocraft-scene3d`), `crates/doc/src/scene3d.rs`, `crates/format/Cargo.toml`, `crates/format/src/manifest.rs`, `crates/format/src/convert.rs`, the format crate's tests; every place that builds a `Scene3dLayer` literal (find them with `grep -rn "Scene3dLayer {" crates apps`)
- Test: `crates/format/src/` test module next to the existing Scene3d round trip

**Interfaces:**
- Consumes: Task 1's `Scene` (built through `Scene::new` and `Mesh::new` on load, so a hostile manifest gets the same checks).
- Produces: `Scene3dLayer { cache: Surface, psd_raw: Option<Arc<Vec<u8>>>, scene: Option<Arc<Scene>>, dirty: bool }`. Phase 1 imports set `scene: None, dirty: false`. `dirty` means "the cache was rendered by PhotoCraft from an edited scene" (phase 4 sets it).
- Manifest: `ContentM::Scene3d` gains `#[serde(default, skip_serializing_if = "Option::is_none")] scene: Option<SceneM>` and `#[serde(default)] dirty: bool`. `SceneM` mirrors `Scene` field by field, except: each mesh's `positions`, `normals`, `uvs` and `indices` are blob hashes of little-endian `f32`/`u32` arrays (`sink.blob`), and each texture is `TextureM { name, surface: SurfaceM, icc: Option<Hash> }`. Enums are tagged by name (`"kind": "point"`), so unknown future kinds fail to load with `FormatError::corrupt` naming the layer rather than silently dropping a light.
- Loading checks each blob's length is a multiple of the element size and matches the declared counts before converting, and the counts against `scene3d::Limits::default()`; then builds with `Mesh::new` / `Scene::new`, mapping `SceneError` to `FormatError::corrupt`.

- [ ] **Step 1: Write the failing tests** in the format crate: `pcraft_round_trips_a_3d_layer_with_its_scene` (a scene with a textured cube, a quad, each light kind, both projections and a ground plane; textures at RGBA8, RGBA16 and RGBA32F; equal after reload, `dirty` kept), `phase1_pcraft_without_scene_still_loads` (a manifest JSON with the phase-1 `Scene3d` shape loads with `scene: None, dirty: false`), `pcraft_rejects_mesh_blob_of_wrong_length`, `pcraft_rejects_index_past_vertices` (hand-edited manifest; `Err`, no panic), `pcraft_rejects_unknown_light_kind`, `pcraft_rejects_over_limit_counts_before_allocating` (declares 2^40 vertices). In `photocraft-doc`: update the existing `scene3d_is_a_3d_layer_with_read_only_pixels` for the new fields.
- [ ] **Step 2:** Run `cargo test -p photocraft-format -p photocraft-doc` and see them fail.
- [ ] **Step 3:** Implement. Update every `Scene3dLayer { … }` literal (io import, tests) with `scene: None, dirty: false`.
- [ ] **Step 4:** Run `cargo test -p photocraft-doc -p photocraft-format -p photocraft-io --lib --tests`, `cargo test -p photocraft-engine --lib -- 3d`, clippy on doc/format/io with `-D warnings`, `cargo xtask layers`, `cargo xtask wasm`.
- [ ] **Step 5:** Commit: `format: store a 3D layer's scene in .pcraft`.

### Task 7: Timing, docs and dev log

**Files:**
- Create: `crates/render3d/examples/render_bench.rs` (renders a synthetic scene of 200,000 triangles, a 1,024² texture, one shadowed infinite light and the ground plane at 6,000 × 4,000, Draft and Final, and prints wall-clock times; no file output)
- Modify: `docs/gaps.md` (G18: phase 2 landed, with the date and the timings), `docs/target-app-parity.md` (the 3D row and a revision entry; no percentage changes), `ROADMAP.md` status line, `docs/architecture.md` (the two crates in the crate map and the layer table), `crates/render3d/README.md` and `crates/scene3d/README.md` (public API, conventions, limits)
- Append: `log/devlog.md` (gitignored)

- [ ] **Step 1:** Run `cargo run --release -p photocraft-render3d --example render_bench` and record Draft and Final times and the machine's core count.
- [ ] **Step 2:** Update the docs above. Status lines get "Last updated 2026-11-19" and a Change entry; revision tables get a row.
- [ ] **Step 3:** Final gate: `cargo test -p photocraft-scene3d -p photocraft-render3d -p photocraft-doc -p photocraft-format -p photocraft-io --lib --tests`, `cargo test -p photocraft-engine --lib`, clippy `--all-targets -- -D warnings` on those crates, `cargo xtask layers`, `cargo xtask wasm`. Put the actual results in the report; list what couldn't run.
- [ ] **Step 4:** Commit: `docs: 3D phase 2, scene model and CPU renderer`.
