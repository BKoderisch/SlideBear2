//! Tests mit einer im Test erzeugten Mini-PPTX (Gemeindeforum-Slide).

use std::io::Write;

use slidebear_core::scene::{ElementKind, HAlign, VAlign};
use slidebear_core::smart;

use super::*;

/// 1x1 PNG, rot.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63,
    0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

fn rels(items: &[(&str, &str, &str)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, ty, target)| format!(r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{ty}" Target="{target}"/>"#))
        .collect();
    format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#
    )
}

#[allow(clippy::too_many_arguments)]
fn text_shape(id: u32, name: &str, y: i64, cy: i64, text: &str, sz: u32, typeface: &str, extra_rpr: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="0" y="{y}"/><a:ext cx="12192000" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"/><a:noFill/></p:spPr>
        <p:txBody><a:bodyPr wrap="square" anchor="ctr"/><a:lstStyle/><a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="de-DE" sz="{sz}"{extra_rpr}><a:solidFill><a:srgbClr val="E6E6E6"/></a:solidFill><a:latin typeface="{typeface}"/></a:rPr><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"#
    )
}

fn build_pptx() -> Vec<u8> {
    let slide = format!(
        r#"<?xml version="1.0"?><p:sld {NS}><p:cSld>
        <p:bg><p:bgPr><a:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></a:blipFill></p:bgPr></p:bg>
        <p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>
        {}
        {}
        <p:sp><p:nvSpPr><p:cNvPr id="4" name="Balken"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
          <p:spPr><a:xfrm><a:off x="0" y="6000000"/><a:ext cx="12192000" cy="858000"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom>
          <a:solidFill><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:solidFill></p:spPr></p:sp>
        <p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="5" name="Tabelle"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr></p:graphicFrame>
        </p:spTree></p:cSld></p:sld>"#,
        text_shape(2, "Titel", 1500000, 1800000, "Gemeindeforum", 16600, "Roboto", ""),
        text_shape(3, "Info", 3500000, 700000, "17.09.2025 | 19:30 Uhr | FeG Adlershof", 4800, "Roboto Light", ""),
    );
    let pres = format!(
        r#"<?xml version="1.0"?><p:presentation {NS}><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#
    );
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#.to_vec(),
        ),
        ("ppt/presentation.xml", pres.into_bytes()),
        ("ppt/_rels/presentation.xml.rels", rels(&[("rId2", "slide", "slides/slide1.xml")]).into_bytes()),
        ("ppt/slides/slide1.xml", slide.into_bytes()),
        ("ppt/slides/_rels/slide1.xml.rels", rels(&[("rIdImg", "image", "../media/image1.png")]).into_bytes()),
        ("ppt/media/image1.png", PNG.to_vec()),
    ];
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in files {
            zip.start_file(name, opts).unwrap();
            zip.write_all(&data).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

#[test]
fn imports_event_slide() {
    let imp = import_bytes(&build_pptx()).unwrap();
    assert_eq!(imp.slides.len(), 1);
    assert_eq!(imp.assets.len(), 1);
    let slide = &imp.slides[0];
    assert_eq!(slide.scene.width, 1920);
    assert_eq!(slide.scene.height, 1080);
    assert!(slide.warnings.iter().any(|w| w.contains("Tabellen")));

    let els = &slide.scene.elements;
    assert_eq!(els.len(), 4, "{els:#?}");

    // Hintergrund
    let ElementKind::Image(bg) = &els[0].kind else { panic!("Hintergrund fehlt") };
    assert_eq!(bg.asset, imp.assets[0].0);
    assert!(els[0].locked);

    // Titel: 166 pt bei 1920 px Breite = 166 * 12700 * 1920 / 12192000 ≈ 332 px
    let ElementKind::Text(title) = &els[1].kind else { panic!() };
    assert_eq!(title.text, "Gemeindeforum");
    assert!((title.size_px - 332.0).abs() < 1.0, "{}", title.size_px);
    assert_eq!(title.align_h, HAlign::Center);
    assert_eq!(title.align_v, VAlign::Middle);
    assert_eq!(title.color, Color::rgb(0xE6, 0xE6, 0xE6));

    // „Roboto Light“ → Roboto, Gewicht 300
    let ElementKind::Text(info) = &els[2].kind else { panic!() };
    assert_eq!(info.font_family, "Roboto");
    assert_eq!(info.weight, 300);

    // Abgerundeter, halbtransparenter Balken
    let ElementKind::Shape(bar) = &els[3].kind else { panic!() };
    assert!(matches!(bar.shape, ShapeKind::RoundedRect { .. }));
    assert_eq!(bar.fill, Some(Color::rgba(0, 0, 0, 128)));

    // Smart-Platzhalter funktionieren direkt auf dem Import
    let d = smart::detect(&slide.scene);
    assert_eq!(d.title.as_deref(), Some("Gemeindeforum"));
    assert_eq!(d.location.as_deref(), Some("FeG Adlershof"));
}

#[test]
fn group_transform_maps_children() {
    let xml = r#"<p:xfrm xmlns:p="p" xmlns:a="a"><a:off x="1000" y="2000"/><a:ext cx="200" cy="200"/><a:chOff x="0" y="0"/><a:chExt cx="100" cy="100"/></p:xfrm>"#;
    let doc = Document::parse(xml).unwrap();
    let xf = group_xf(doc.root_element(), IDENTITY);
    assert_eq!((xf.ox + 50.0 * xf.sx, xf.oy + 50.0 * xf.sy), (1100.0, 2100.0));
}

#[test]
fn path_resolution() {
    assert_eq!(resolve("ppt/slides", "../media/image1.png"), "ppt/media/image1.png");
    assert_eq!(resolve("ppt", "slides/slide1.xml"), "ppt/slides/slide1.xml");
    assert_eq!(resolve("ppt/slides", "/ppt/media/a.png"), "ppt/media/a.png");
}

#[test]
fn weight_from_face_name() {
    assert_eq!(split_weight("Roboto Light"), ("Roboto".into(), 300));
    assert_eq!(split_weight("Arial"), ("Arial".into(), 400));
}
