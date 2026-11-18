//! Photoshop 3D layers: telling a `PxSc` (pixel source) block that holds a 3D scene from one
//! that does not.
//!
//! Phase 1 reads only the top-level `pixelSourceType` key; the scene itself is kept verbatim.

use craft_psd::descriptor::{Value, VersionedDescriptor};

/// `pixelSourceType` of a 3D scene: the four bytes `3dPS`, as Photoshop writes it.
const THREE_D_PS: i32 = 0x3364_5053;

/// What a layer's `PxSc` block holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PixelSource {
    /// A 3D scene.
    ThreeD,
    /// Any other readable pixel source.
    Other,
    /// The block does not parse as a versioned descriptor.
    Unreadable,
}

pub(crate) fn classify_pixel_source(pxsc: &[u8]) -> PixelSource {
    match VersionedDescriptor::parse_prefix(pxsc) {
        Ok((v, _)) if matches!(v.descriptor.get("pixelSourceType"), Some(Value::Integer(THREE_D_PS))) => PixelSource::ThreeD,
        Ok(_) => PixelSource::Other,
        Err(_) => PixelSource::Unreadable,
    }
}

#[cfg(test)]
mod tests;
