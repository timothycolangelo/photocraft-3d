//! 3D layers (Photoshop's `PxSc` layers).
//!
//! Phase 1 keeps only what is needed to open, show and save such a layer without loss: the last
//! render Photoshop stored with it (`cache`) and the raw `PxSc` block bytes (`psd_raw`). The scene
//! itself is not decoded, so the layer is displayed from `cache` and is not editable as pixels.

use std::sync::Arc;

use photocraft_raster::Surface;

/// A 3D layer: the stored render plus the verbatim scene block.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene3dLayer {
    /// Photoshop's (later: PhotoCraft's) last render of the scene, in document coordinates.
    pub cache: Surface,
    /// The layer's `PxSc` block data, written back verbatim while the scene is unchanged.
    pub psd_raw: Option<Arc<Vec<u8>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layer, LayerContent, PixelFormat};

    #[test]
    fn scene3d_is_a_3d_layer_with_read_only_pixels() {
        let mut layer = Layer::new("3D", LayerContent::Scene3d(Scene3dLayer { cache: Surface::new(PixelFormat::RGBA8), psd_raw: None }));
        assert_eq!(layer.content.kind_name(), "3D");
        assert_eq!(layer.content.article(), "a");
        assert!(layer.surface().is_some());
        assert!(layer.surface_mut().is_none());
    }
}
