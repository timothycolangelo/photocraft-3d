//! A 3D layer keeps its stored render and raw scene block through `.pcraft`.

use std::sync::Arc;

use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, Scene3dLayer, Size};
use photocraft_format::*;
use photocraft_geom::Rect;
use photocraft_raster::{Surface, from_rgba};

fn round_trip(sample: SampleType) {
    let fmt = PixelFormat { mode: ColorMode::Rgb, sample, alpha: true };
    let mut cache = Surface::new(fmt);
    cache.fill_rect(Rect::new(1, 2, 1, 1), &from_rgba(&fmt, [1.0, 0.0, 0.5, 1.0]));
    let mut doc = Document::new("3D", Size::new(4, 4), ColorMode::Rgb, sample);
    doc.layers.push(Layer::new("3D", LayerContent::Scene3d(Scene3dLayer { cache: cache.clone(), psd_raw: Some(Arc::new(vec![1, 2, 3])) })));

    let bytes = save_to_bytes(&doc, &SaveOptions::default()).unwrap();
    let back = load_from_bytes(&bytes).unwrap();
    let Some(LayerContent::Scene3d(s)) = back.layers.last().map(|l| &l.content) else { panic!("a 3D layer") };
    assert_eq!(s.cache.rgba(1, 2), cache.rgba(1, 2));
    assert_eq!(s.psd_raw.as_deref().map(Vec::as_slice), Some(&[1u8, 2, 3][..]));
    assert_eq!(back, doc);
}

#[test]
fn scene3d_layer_round_trips_in_pcraft() {
    round_trip(SampleType::U8);
    round_trip(SampleType::U16);
}
