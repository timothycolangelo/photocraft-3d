use super::*;
use craft_psd::descriptor::{Descriptor, Value, VersionedDescriptor};
use craft_psd::{ColorMode as PsdMode, Compression, PsdFile, TaggedBlock, Version};
use photocraft_doc::{Layer, LayerContent};

const THREE_D: i32 = 0x3364_5053;

/// Padded to a multiple of 4 inside the block, as Photoshop writes layer blocks (the writer
/// pads an odd-length one, so an unpadded fixture would not come back byte-identical).
fn pxsc(kind: i32) -> Vec<u8> {
    let mut v = VersionedDescriptor::new(Descriptor::new("PixelSource").with("pixelSourceType", Value::Integer(kind)).with("descVersion", Value::Integer(2)))
        .to_bytes();
    v.resize(v.len().next_multiple_of(4), 0);
    v
}

/// A layered file whose layer named `target` carries `data` as `PxSc`, plus a global `PxSD`.
fn file_with(version: Version, depth: u16, target: &str, data: Vec<u8>) -> PsdFile {
    let mut f = craft_psd::testgen::layered(version, PsdMode::Rgb, depth, Compression::Rle);
    let li = f.layer_info.as_mut().unwrap();
    let rec = li.layers.iter_mut().find(|r| r.name() == target).unwrap();
    rec.blocks.push(TaggedBlock::new(*b"PxSc", data));
    f.global_blocks.push(TaggedBlock::new(*b"PxSD", vec![0xA5; 64]));
    f
}

fn open(f: &PsdFile) -> crate::ImportResult {
    crate::import("t.psd", &f.to_bytes().unwrap()).unwrap()
}

fn all<'a>(layers: &'a [Layer], out: &mut Vec<&'a Layer>) {
    for l in layers {
        out.push(l);
        if let Some(c) = l.children() {
            all(c, out);
        }
    }
}

fn flat(doc: &photocraft_doc::Document) -> Vec<&Layer> {
    let mut v = Vec::new();
    all(&doc.layers, &mut v);
    v
}

fn threed(doc: &photocraft_doc::Document) -> Vec<&Layer> {
    flat(doc).into_iter().filter(|l| matches!(l.content, LayerContent::Scene3d(_))).collect()
}

fn block<'a>(rec_blocks: &'a [TaggedBlock], key: &[u8; 4]) -> Option<&'a TaggedBlock> {
    rec_blocks.iter().find(|b| &b.key == key)
}

#[test]
fn pxsc_type_3dps_is_detected() {
    assert_eq!(classify_pixel_source(&pxsc(THREE_D)), PixelSource::ThreeD);
}

#[test]
fn other_pixel_source_types_are_not() {
    assert_eq!(classify_pixel_source(&pxsc(0x7669_6420)), PixelSource::Other);
    let no_type = VersionedDescriptor::new(Descriptor::new("PixelSource").with("descVersion", Value::Integer(2))).to_bytes();
    assert_eq!(classify_pixel_source(&no_type), PixelSource::Other);
    assert_eq!(classify_pixel_source(&[0, 0, 0, 16, 0xFF]), PixelSource::Unreadable);
    assert_eq!(classify_pixel_source(&[]), PixelSource::Unreadable);
}

#[test]
fn three_d_layer_opens_as_scene3d() {
    for version in [Version::Psd, Version::Psb] {
        for depth in [8u16, 16] {
            let plain = craft_psd::testgen::layered(version, PsdMode::Rgb, depth, Compression::Rle);
            let plain = open(&plain);
            let r = open(&file_with(version, depth, "Background", pxsc(THREE_D)));
            assert!(r.warnings.iter().all(|w| !w.contains("3D")), "{:?}", r.warnings);
            let doc = &r.document;
            let layers = threed(doc);
            assert_eq!(layers.len(), 1, "{version:?} {depth}");
            let LayerContent::Scene3d(s) = &layers[0].content else { panic!() };
            assert_eq!(s.psd_raw.as_deref(), Some(&pxsc(THREE_D)));
            let base = plain.document.layers.iter().find(|l| l.name == "Background").unwrap().surface().unwrap();
            assert_eq!(s.cache.read_region(doc.bounds()), base.read_region(plain.document.bounds()));
        }
    }
}

#[test]
fn untouched_three_d_file_saves_blocks_verbatim() {
    for version in [Version::Psd, Version::Psb] {
        let data = pxsc(THREE_D);
        let r = open(&file_with(version, 8, "Background", data.clone()));
        let opts = crate::PsdExportOptions { force_psb: version == Version::Psb, ..Default::default() };
        let out = crate::psd_export::document_to_psd_with(&r.document, &opts).unwrap().0;
        assert_eq!(out.written_version(), version);
        let back = PsdFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
        let recs = &back.layer_info.as_ref().unwrap().layers;
        let rec = recs.iter().find(|r| r.name() == "Background").unwrap();
        assert_eq!(block(&rec.blocks, b"PxSc").unwrap().data, data, "{version:?}");
        assert_eq!(rec.blocks.iter().filter(|b| &b.key == b"PxSc").count(), 1);
        let g = block(&back.global_blocks, b"PxSD").unwrap();
        assert_eq!(g.data, vec![0xA5; 64], "{version:?}");
        if version == Version::Psb {
            let bytes = out.to_bytes().unwrap();
            assert!(bytes.windows(8).any(|w| w == b"8B64PxSD"), "PSB writes PxSD with an 8B64 signature");
        }
    }
}

#[test]
fn duplicated_three_d_layer_stays_three_d() {
    let mut r = open(&file_with(Version::Psd, 8, "Background", pxsc(THREE_D)));
    let dup = r.document.layers.iter().find(|l| matches!(l.content, LayerContent::Scene3d(_))).unwrap().duplicate();
    r.document.layers.push(dup);
    let out = crate::psd_export::document_to_psd(&r.document).unwrap();
    let back = crate::import("t.psd", &out.to_bytes().unwrap()).unwrap();
    assert_eq!(threed(&back.document).len(), 2);
}

#[test]
fn three_d_layer_in_a_group_round_trips() {
    let data = pxsc(THREE_D);
    let r = open(&file_with(Version::Psd, 16, "In outer", data.clone()));
    assert_eq!(threed(&r.document).len(), 1);
    assert!(r.document.layers.iter().all(|l| !matches!(l.content, LayerContent::Scene3d(_))), "it is nested");
    let out = crate::psd_export::document_to_psd(&r.document).unwrap();
    let back = PsdFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let rec = back.layer_info.as_ref().unwrap().layers.iter().find(|r| r.name() == "In outer").unwrap();
    assert_eq!(block(&rec.blocks, b"PxSc").unwrap().data, data);
    assert_eq!(threed(&crate::import("t.psd", &out.to_bytes().unwrap()).unwrap().document).len(), 1);
}

#[test]
fn malformed_pxsc_opens_as_pixels_with_warning() {
    for data in [pxsc(THREE_D)[..7].to_vec(), Vec::new()] {
        let r = open(&file_with(Version::Psd, 8, "Background", data.clone()));
        assert!(r.warnings.iter().any(|w| w.contains("its 3D data could not be read")), "{:?}", r.warnings);
        let l = r.document.layers.iter().find(|l| l.name == "Background").unwrap();
        assert!(matches!(l.content, LayerContent::Raster(_)));
        assert!(l.psd_blocks.iter().any(|(k, d)| k == b"PxSc" && **d == data));
    }
}

#[test]
fn other_pixel_source_stays_a_pixel_layer_without_warning() {
    let r = open(&file_with(Version::Psd, 8, "Background", pxsc(0x7669_6420)));
    assert!(r.warnings.iter().all(|w| !w.contains("3D")), "{:?}", r.warnings);
    let l = r.document.layers.iter().find(|l| l.name == "Background").unwrap();
    assert!(matches!(l.content, LayerContent::Raster(_)));
    assert!(l.psd_blocks.iter().any(|(k, _)| k == b"PxSc"));
}
