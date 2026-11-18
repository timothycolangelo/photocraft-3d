# Feature naming

PhotoCraft names its own raw-processing workspace **RAW Develop**, the adjustment **RAW Develop Filter**, and its raw preferences **RAW Defaults**. All 34 shipped non-English catalogs use the new keys and localized labels.

## Compatibility boundary

The `filter.cameraRaw` command, raw preference IDs, Rust type names and serialized settings remain stable. Existing documents retain their own saved filter names. New filters get the neutral display name.

Adobe Camera Raw PSD/action descriptor class names, XMP namespaces and the settings-file compatibility descriptions are unchanged. Import/export mapping code and attribution are unchanged.

A Photoshop action's step name is stored in the action file, so a saved Camera Raw step keeps Photoshop's "Camera Raw Filter" wording and a loaded action round-trips unchanged. Only PhotoCraft's own step labels, menus and windows say RAW Develop Filter.

## Validation before readiness

Review menu and dialog layout in the supported interfaces, run the engine/UI tests and the existing PSD/action/XMP round-trip tests, and complete the normal platform gates. This is a feature naming change, not a legal determination.

The catalog cleanup removes 136 dormant rows for branded composer labels, PDF presets, old Photoshop defaults and the unused raw-opening label. No active non-localization call sites use those keys. Current PhotoCraft defaults and required compatibility descriptions remain intact. Regression guards cover production Rust display literals and catalog keys; explicit exceptions document interchange descriptions and identifiers.

## 3D layers (2026-11-18)

PhotoCraft labels a layer that holds a stored three-dimensional scene **3D**, and its menu item **Layer › Rasterize › 3D** (command id `layer.rasterize.3d`). Both are ordinary descriptive words that several unrelated products use for the same thing, like `Layers` and `Crop`, so under `standards/naming.md` (RFC-0029) neither is another company's distinctive feature name and no names-list entry is needed. The layer kind is `Scene3d` in code. The PSD block keys `PxSc` and `PxSD` are file-format compatibility names, kept as is and never shown in the interface.
