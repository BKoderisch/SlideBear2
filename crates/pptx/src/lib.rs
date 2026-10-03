//! PPTX-Import: liest PowerPoint-Dateien direkt (ZIP + XML) und erzeugt bearbeitbare Szenen
//! (Text, Bilder, Formen) statt Rasterbildern.

mod color;
mod xml;

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;

use roxmltree::{Document, Node};
use slidebear_core::Scene;
use slidebear_core::assets::content_name;
use slidebear_core::scene::{
    Color, Element, ElementKind, HAlign, ImageFit, ImageStyle, Outline, Rect, Shadow, ShapeKind, ShapeStyle, Stroke, TextStyle, VAlign,
};

use color::{Theme, parse_fill_color};
use xml::{attr, attr_bool, attr_i64, child, children, path, rel_attr};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Datei konnte nicht gelesen werden: {0}")]
    Io(#[from] std::io::Error),
    #[error("Keine gültige PPTX-Datei: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Fehlerhafte PPTX-Struktur: {0}")]
    Structure(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct ImportedSlide {
    /// 1-basierte Foliennummer wie in PowerPoint.
    pub number: usize,
    pub hidden: bool,
    pub scene: Scene,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Import {
    pub slides: Vec<ImportedSlide>,
    /// Bilddateien, die in den Asset-Ordner geschrieben werden müssen: (Dateiname, Daten).
    pub assets: Vec<(String, Vec<u8>)>,
    /// In der Präsentation verwendete Schriftarten.
    pub fonts: Vec<String>,
}

pub fn import_file(path: &Path) -> Result<Import> {
    import_bytes(&std::fs::read(path)?)
}

pub fn import_bytes(data: &[u8]) -> Result<Import> {
    let pkg = Package::open(data)?;
    let pres_xml = pkg.text("ppt/presentation.xml").ok_or_else(|| Error::Structure("presentation.xml fehlt".into()))?;
    let pres = Document::parse(&pres_xml).map_err(|e| Error::Structure(e.to_string()))?;
    let root = pres.root_element();

    let sld_sz = child(root, "sldSz");
    let cx = sld_sz.and_then(|n| attr_i64(n, "cx")).unwrap_or(12_192_000) as f32;
    let cy = sld_sz.and_then(|n| attr_i64(n, "cy")).unwrap_or(6_858_000) as f32;
    let width = slidebear_core::scene::DEFAULT_WIDTH;
    let height = ((cy / cx) * width as f32).round() as u32;
    let scale = width as f32 / cx;

    let rels = pkg.rels("ppt/presentation.xml");
    let slide_parts: Vec<String> = child(root, "sldIdLst")
        .map(|l| children(l).filter_map(|s| rel_attr(s, "id")).filter_map(|id| rels.get(id).cloned()).collect())
        .unwrap_or_default();

    let theme = pkg
        .files
        .keys()
        .filter(|k| k.starts_with("ppt/theme/theme"))
        .min()
        .and_then(|k| pkg.text(k))
        .map(|x| Theme::parse(&x))
        .unwrap_or_default();

    let mut out = Import::default();
    for (i, part) in slide_parts.iter().enumerate() {
        let mut ctx =
            Ctx { pkg: &pkg, theme: &theme, scale, width, height, assets: &mut out.assets, fonts: &mut out.fonts, warnings: Vec::new() };
        match ctx.slide(part) {
            Ok((scene, hidden)) => {
                let warnings = std::mem::take(&mut ctx.warnings);
                out.slides.push(ImportedSlide { number: i + 1, hidden, scene, warnings });
            }
            Err(e) => out.slides.push(ImportedSlide {
                number: i + 1,
                hidden: false,
                scene: Scene { width, height, elements: Vec::new() },
                warnings: vec![format!("Folie konnte nicht gelesen werden: {e}")],
            }),
        }
    }
    out.fonts.sort();
    out.fonts.dedup();
    Ok(out)
}

struct Package {
    files: HashMap<String, Vec<u8>>,
}

impl Package {
    fn open(data: &[u8]) -> Result<Self> {
        let mut zip = zip::ZipArchive::new(Cursor::new(data))?;
        let mut files = HashMap::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let mut buf = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut buf)?;
            files.insert(f.name().trim_start_matches('/').to_string(), buf);
        }
        Ok(Self { files })
    }

    fn text(&self, part: &str) -> Option<String> {
        self.files.get(part).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// Beziehungen eines Parts: rId → absoluter Pfad im Paket.
    fn rels(&self, part: &str) -> HashMap<String, String> {
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let rels_part = format!("{dir}/_rels/{file}.rels");
        let Some(xml) = self.text(&rels_part) else { return HashMap::new() };
        let Ok(doc) = Document::parse(&xml) else { return HashMap::new() };
        doc.root_element()
            .children()
            .filter(Node::is_element)
            .filter(|r| r.attribute("TargetMode") != Some("External"))
            .filter_map(|r| Some((r.attribute("Id")?.to_string(), resolve(dir, r.attribute("Target")?))))
            .collect()
    }

    /// Erstes Beziehungsziel mit passendem Pfad, z. B. das Layout einer Folie.
    fn rel_target(&self, part: &str, contains: &str) -> Option<String> {
        self.rels(part).into_values().find(|t| t.contains(contains))
    }
}

fn resolve(base_dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = base_dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Text-Eigenschaften mit Vererbung (Run → Shape → Layout → Master).
#[derive(Debug, Clone, Default)]
struct RunProps {
    size_pt: Option<f32>,
    bold: Option<bool>,
    italic: Option<bool>,
    typeface: Option<String>,
    color: Option<Color>,
    shadow: Option<Shadow>,
    outline: Option<Outline>,
}

impl RunProps {
    fn or(self, other: &RunProps) -> RunProps {
        RunProps {
            size_pt: self.size_pt.or(other.size_pt),
            bold: self.bold.or(other.bold),
            italic: self.italic.or(other.italic),
            typeface: self.typeface.or_else(|| other.typeface.clone()),
            color: self.color.or(other.color),
            shadow: self.shadow.or_else(|| other.shadow.clone()),
            outline: self.outline.or_else(|| other.outline.clone()),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ParaProps {
    align: Option<HAlign>,
    line_height: Option<f32>,
}

/// Platzhalter aus Layout oder Master, von dem Folien-Platzhalter Position und Stil erben.
#[derive(Debug, Clone)]
struct Inherited {
    kind: String,
    idx: Option<String>,
    frame: Option<Rect>,
    run: RunProps,
    para: ParaProps,
    anchor: Option<VAlign>,
}

#[derive(Clone, Copy)]
struct Xf {
    ox: f32,
    oy: f32,
    sx: f32,
    sy: f32,
}

const IDENTITY: Xf = Xf { ox: 0.0, oy: 0.0, sx: 1.0, sy: 1.0 };

struct Ctx<'a> {
    pkg: &'a Package,
    theme: &'a Theme,
    /// Slide-Pixel pro EMU.
    scale: f32,
    width: u32,
    height: u32,
    assets: &'a mut Vec<(String, Vec<u8>)>,
    fonts: &'a mut Vec<String>,
    warnings: Vec<String>,
}

fn norm_kind(kind: &str) -> &str {
    match kind {
        "ctrTitle" => "title",
        "subTitle" | "obj" => "body",
        k => k,
    }
}

impl Ctx<'_> {
    fn warn(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if !self.warnings.contains(&msg) {
            self.warnings.push(msg);
        }
    }

    fn px(&self, emu: f32) -> f32 {
        emu * self.scale
    }

    fn slide(&mut self, part: &str) -> Result<(Scene, bool)> {
        let xml = self.pkg.text(part).ok_or_else(|| Error::Structure(format!("{part} fehlt")))?;
        let doc = Document::parse(&xml).map_err(|e| Error::Structure(e.to_string()))?;
        let root = doc.root_element();
        let hidden = attr(root, "show") == Some("0");

        let layout_part = self.pkg.rel_target(part, "slideLayouts/");
        let master_part = layout_part.as_deref().and_then(|l| self.pkg.rel_target(l, "slideMasters/"));
        let layout_xml = layout_part.as_deref().and_then(|p| self.pkg.text(p));
        let master_xml = master_part.as_deref().and_then(|p| self.pkg.text(p));
        let layout_doc = layout_xml.as_deref().and_then(|x| Document::parse(x).ok());
        let master_doc = master_xml.as_deref().and_then(|x| Document::parse(x).ok());

        let master_styles = master_doc.as_ref().map(|d| self.master_text_styles(d.root_element())).unwrap_or_default();
        let master_phs = master_doc.as_ref().map(|d| self.placeholders(d.root_element())).unwrap_or_default();
        let layout_phs = layout_doc.as_ref().map(|d| self.placeholders(d.root_element())).unwrap_or_default();

        let mut elements = Vec::new();

        // Hintergrund: Folie → Layout → Master
        let bg_sources = [
            Some((root, part.to_string())),
            layout_doc.as_ref().zip(layout_part.clone()).map(|(d, p)| (d.root_element(), p)),
            master_doc.as_ref().zip(master_part.clone()).map(|(d, p)| (d.root_element(), p)),
        ];
        for (node, owner) in bg_sources.into_iter().flatten() {
            if let Some(bg) = path(node, &["cSld", "bg"]) {
                if let Some(el) = self.background(bg, &owner) {
                    elements.push(el);
                }
                break;
            }
        }

        // Nicht-Platzhalter-Deko aus Master und Layout (z. B. Logos), sofern nicht ausgeblendet
        let show_master = attr_bool(root, "showMasterSp").unwrap_or(true);
        if show_master {
            if let (Some(d), Some(p)) = (master_doc.as_ref(), master_part.as_deref()) {
                let layout_shows = layout_doc.as_ref().and_then(|l| attr_bool(l.root_element(), "showMasterSp")).unwrap_or(true);
                if layout_shows {
                    self.decoration(d.root_element(), p, &mut elements);
                }
            }
            if let (Some(d), Some(p)) = (layout_doc.as_ref(), layout_part.as_deref()) {
                self.decoration(d.root_element(), p, &mut elements);
            }
        }

        let inherit = Inheritance { layout: &layout_phs, master: &master_phs, styles: &master_styles };
        if let Some(tree) = path(root, &["cSld", "spTree"]) {
            self.walk(tree, part, IDENTITY, Some(&inherit), &mut elements);
        }

        Ok((Scene { width: self.width, height: self.height, elements }, hidden))
    }

    fn decoration(&mut self, root: Node, part: &str, out: &mut Vec<Element>) {
        let Some(tree) = path(root, &["cSld", "spTree"]) else { return };
        let mut tmp = Vec::new();
        self.walk(tree, part, IDENTITY, None, &mut tmp);
        for mut e in tmp {
            e.locked = false;
            out.push(e);
        }
    }

    fn background(&mut self, bg: Node, owner: &str) -> Option<Element> {
        let full = Rect::new(0.0, 0.0, self.width as f32, self.height as f32);
        if let Some(pr) = child(bg, "bgPr") {
            if let Some(blip) = path(pr, &["blipFill", "blip"]) {
                let asset = self.blip_asset(blip, owner)?;
                let mut e =
                    Element::new("Hintergrund", full, ElementKind::Image(ImageStyle { asset, fit: ImageFit::Cover, filters: Vec::new() }));
                e.locked = true;
                return Some(e);
            }
            if let Some(fill) = child(pr, "solidFill") {
                return Some(self.bg_rect(full, parse_fill_color(fill, self.theme)?));
            }
            if let Some(grad) = child(pr, "gradFill") {
                self.warn("Farbverlauf im Hintergrund durch Einzelfarbe ersetzt");
                let stop = grad.descendants().find(|n| n.tag_name().name() == "gs")?;
                return Some(self.bg_rect(full, parse_fill_color(stop, self.theme)?));
            }
        }
        if let Some(r) = child(bg, "bgRef") {
            return Some(self.bg_rect(full, parse_fill_color(r, self.theme)?));
        }
        None
    }

    fn bg_rect(&self, frame: Rect, color: Color) -> Element {
        let mut e =
            Element::new("Hintergrund", frame, ElementKind::Shape(ShapeStyle { shape: ShapeKind::Rect, fill: Some(color), stroke: None }));
        e.locked = true;
        e
    }

    /// Speichert das Bild einer `a:blip` als Asset, liefert den Asset-Namen.
    fn blip_asset(&mut self, blip: Node, owner: &str) -> Option<String> {
        let id = rel_attr(blip, "embed")?;
        let target = self.pkg.rels(owner).get(id)?.clone();
        let data = self.pkg.files.get(&target)?;
        let ext = target.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
        if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff") {
            self.warn(format!("Bildformat .{ext} wird nicht unterstützt und wurde übersprungen"));
            return None;
        }
        let name = content_name(data, &ext);
        if !self.assets.iter().any(|(n, _)| *n == name) {
            self.assets.push((name.clone(), data.clone()));
        }
        Some(name)
    }

    fn frame(&self, xfrm: Node, xf: Xf) -> Option<Rect> {
        let off = child(xfrm, "off")?;
        let ext = child(xfrm, "ext")?;
        let (x, y) = (attr_i64(off, "x")? as f32, attr_i64(off, "y")? as f32);
        let (w, h) = (attr_i64(ext, "cx")? as f32, attr_i64(ext, "cy")? as f32);
        Some(Rect::new(self.px(xf.ox + x * xf.sx), self.px(xf.oy + y * xf.sy), self.px(w * xf.sx), self.px(h * xf.sy)))
    }

    fn walk(&mut self, tree: Node, part: &str, xf: Xf, inherit: Option<&Inheritance>, out: &mut Vec<Element>) {
        for n in children(tree) {
            let hidden = n.descendants().find(|c| c.tag_name().name() == "cNvPr").and_then(|c| attr_bool(c, "hidden")).unwrap_or(false);
            if hidden {
                continue;
            }
            match n.tag_name().name() {
                "sp" => self.shape(n, part, xf, inherit, out),
                "pic" => self.picture(n, part, xf, out),
                "grpSp" => {
                    if let Some(gx) = path(n, &["grpSpPr", "xfrm"]) {
                        self.walk(n, part, group_xf(gx, xf), inherit, out);
                    } else {
                        self.walk(n, part, xf, inherit, out);
                    }
                }
                "graphicFrame" => self.warn("Tabellen, Diagramme und SmartArt wurden übersprungen"),
                "cxnSp" => self.warn("Verbindungslinien wurden übersprungen"),
                "AlternateContent" => {
                    if let Some(fb) = child(n, "Fallback") {
                        self.walk(fb, part, xf, inherit, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn name_of(n: Node, fallback: &str) -> String {
        n.descendants()
            .find(|c| c.tag_name().name() == "cNvPr")
            .and_then(|c| attr(c, "name"))
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback)
            .to_string()
    }

    fn picture(&mut self, n: Node, part: &str, xf: Xf, out: &mut Vec<Element>) {
        let Some(frame) = path(n, &["spPr", "xfrm"]).and_then(|x| self.frame(x, xf)) else { return };
        let Some(blip) = path(n, &["blipFill", "blip"]) else { return };
        if path(n, &["blipFill", "srcRect"]).is_some_and(|s| s.attributes().len() > 0) {
            self.warn("Zugeschnittene Bilder werden ungeschnitten importiert");
        }
        self.check_rotation(n);
        let Some(asset) = self.blip_asset(blip, part) else { return };
        let mut e = Element::new(
            Self::name_of(n, "Bild"),
            frame,
            ElementKind::Image(ImageStyle { asset, fit: ImageFit::Stretch, filters: Vec::new() }),
        );
        if let Some(a) = child(blip, "alphaModFix").and_then(|a| attr_i64(a, "amt")) {
            e.opacity = (a as f32 / 100_000.0).clamp(0.0, 1.0);
        }
        out.push(e);
    }

    fn check_rotation(&mut self, n: Node) {
        if path(n, &["spPr", "xfrm"]).and_then(|x| attr_i64(x, "rot")).is_some_and(|r| r != 0) {
            self.warn("Gedrehte Elemente werden ungedreht importiert");
        }
    }

    fn shape(&mut self, n: Node, part: &str, xf: Xf, inherit: Option<&Inheritance>, out: &mut Vec<Element>) {
        let ph = path(n, &["nvSpPr", "nvPr", "ph"]);
        let ph_kind = ph.map(|p| attr(p, "type").unwrap_or("body").to_string());
        let ph_idx = ph.and_then(|p| attr(p, "idx")).map(str::to_string);

        // Platzhalter auf Layout/Master sind nur Vorlagen, ohne eigenen Inhalt auf der Folie
        if inherit.is_none() && ph.is_some() {
            return;
        }
        let inherited = match (inherit, &ph_kind) {
            (Some(i), Some(kind)) => i.find(kind, ph_idx.as_deref()),
            _ => Vec::new(),
        };

        let frame = path(n, &["spPr", "xfrm"]).and_then(|x| self.frame(x, xf)).or_else(|| inherited.iter().find_map(|i| i.frame));
        let Some(frame) = frame else { return };
        self.check_rotation(n);
        let name = Self::name_of(n, "Form");

        if let Some(sp_pr) = child(n, "spPr") {
            if let Some(blip) = path(sp_pr, &["blipFill", "blip"]) {
                if let Some(asset) = self.blip_asset(blip, part) {
                    out.push(Element::new(
                        name.clone(),
                        frame,
                        ElementKind::Image(ImageStyle { asset, fit: ImageFit::Cover, filters: Vec::new() }),
                    ));
                }
            } else if let Some(style) = self.shape_style(sp_pr, frame) {
                out.push(Element::new(name.clone(), frame, ElementKind::Shape(style)));
            }
        }

        if let Some(body) = child(n, "txBody") {
            let styles = inherit.map(|i| i.styles);
            if let Some(el) = self.text(body, frame, &name, ph_kind.as_deref(), &inherited, styles) {
                out.push(el);
            }
        }
    }

    fn shape_style(&mut self, sp_pr: Node, frame: Rect) -> Option<ShapeStyle> {
        let fill = if let Some(f) = child(sp_pr, "solidFill") {
            parse_fill_color(f, self.theme)
        } else if let Some(g) = child(sp_pr, "gradFill") {
            self.warn("Farbverläufe durch Einzelfarbe ersetzt");
            g.descendants().find(|c| c.tag_name().name() == "gs").and_then(|gs| parse_fill_color(gs, self.theme))
        } else {
            None
        };
        let stroke = child(sp_pr, "ln").and_then(|ln| {
            let color = parse_fill_color(child(ln, "solidFill")?, self.theme)?;
            let w = attr_i64(ln, "w").unwrap_or(12_700) as f32;
            Some(Stroke { color, width: self.px(w).max(1.0) })
        });
        if fill.is_none() && stroke.is_none() {
            return None;
        }
        let prst = path(sp_pr, &["prstGeom"]).and_then(|g| attr(g, "prst")).unwrap_or("rect");
        let shape = match prst {
            "rect" => ShapeKind::Rect,
            "ellipse" => ShapeKind::Ellipse,
            "roundRect" => {
                let adj = path(sp_pr, &["prstGeom", "avLst", "gd"])
                    .and_then(|g| attr(g, "fmla"))
                    .and_then(|f| f.strip_prefix("val "))
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(16_667.0);
                ShapeKind::RoundedRect { radius: frame.w.min(frame.h) * adj / 100_000.0 }
            }
            other => {
                self.warn(format!("Form \"{other}\" als Rechteck importiert"));
                ShapeKind::Rect
            }
        };
        Some(ShapeStyle { shape, fill, stroke })
    }

    fn run_props(&self, rpr: Node) -> RunProps {
        let typeface = child(rpr, "latin").and_then(|l| attr(l, "typeface")).map(|t| match t {
            "+mj-lt" => self.theme.major_font.clone().unwrap_or_default(),
            "+mn-lt" => self.theme.minor_font.clone().unwrap_or_default(),
            t => t.to_string(),
        });
        let shadow = path(rpr, &["effectLst", "outerShdw"]).and_then(|s| {
            let color = parse_fill_color(s, self.theme)?;
            let dist = self.px(attr_i64(s, "dist").unwrap_or(0) as f32);
            let dir = attr_i64(s, "dir").unwrap_or(0) as f32 / 60_000.0;
            let blur = self.px(attr_i64(s, "blurRad").unwrap_or(0) as f32) / 2.0;
            let rad = dir.to_radians();
            Some(Shadow { color, offset_x: dist * rad.cos(), offset_y: dist * rad.sin(), blur })
        });
        let outline = child(rpr, "ln").and_then(|ln| {
            let color = parse_fill_color(child(ln, "solidFill")?, self.theme)?;
            let w = self.px(attr_i64(ln, "w").unwrap_or(12_700) as f32);
            Some(Outline { color, width: (w / 2.0).max(0.5) })
        });
        RunProps {
            size_pt: attr_i64(rpr, "sz").map(|s| s as f32 / 100.0),
            bold: attr_bool(rpr, "b"),
            italic: attr_bool(rpr, "i"),
            typeface: typeface.filter(|t| !t.is_empty()),
            color: child(rpr, "solidFill").and_then(|f| parse_fill_color(f, self.theme)),
            shadow,
            outline,
        }
    }

    fn para_props(&self, ppr: Node) -> ParaProps {
        ParaProps {
            align: attr(ppr, "algn").map(|a| match a {
                "ctr" => HAlign::Center,
                "r" => HAlign::Right,
                _ => HAlign::Left,
            }),
            line_height: path(ppr, &["lnSpc", "spcPct"]).and_then(|s| attr_i64(s, "val")).map(|v| v as f32 / 100_000.0 * 1.2),
        }
    }

    /// Erste Ebene einer Listen-Formatvorlage (`a:lstStyle`, `p:titleStyle` …).
    fn level1(&self, list: Node) -> (RunProps, ParaProps) {
        match child(list, "lvl1pPr") {
            Some(l) => (child(l, "defRPr").map(|r| self.run_props(r)).unwrap_or_default(), self.para_props(l)),
            None => Default::default(),
        }
    }

    fn placeholders(&self, root: Node) -> Vec<Inherited> {
        let Some(tree) = path(root, &["cSld", "spTree"]) else { return Vec::new() };
        tree.descendants()
            .filter(|n| n.tag_name().name() == "sp")
            .filter_map(|sp| {
                let ph = path(sp, &["nvSpPr", "nvPr", "ph"])?;
                let body = child(sp, "txBody");
                let (run, para) = body.and_then(|b| child(b, "lstStyle")).map(|l| self.level1(l)).unwrap_or_default();
                Some(Inherited {
                    kind: attr(ph, "type").unwrap_or("body").to_string(),
                    idx: attr(ph, "idx").map(str::to_string),
                    frame: path(sp, &["spPr", "xfrm"]).and_then(|x| self.frame(x, IDENTITY)),
                    run,
                    para,
                    anchor: body.and_then(|b| child(b, "bodyPr")).and_then(anchor),
                })
            })
            .collect()
    }

    fn master_text_styles(&self, root: Node) -> MasterStyles {
        let get = |name: &str| path(root, &["txStyles", name]).map(|n| self.level1(n)).unwrap_or_default();
        MasterStyles { title: get("titleStyle"), body: get("bodyStyle"), other: get("otherStyle") }
    }

    fn text(
        &mut self,
        body: Node,
        frame: Rect,
        name: &str,
        ph_kind: Option<&str>,
        inherited: &[&Inherited],
        styles: Option<&MasterStyles>,
    ) -> Option<Element> {
        let mut lines = Vec::new();
        let mut first_run: Option<RunProps> = None;
        let mut first_para: Option<ParaProps> = None;
        let mut sizes = Vec::new();

        for p in children(body).filter(|c| c.tag_name().name() == "p") {
            let mut line = String::new();
            let para = child(p, "pPr").map(|pp| self.para_props(pp)).unwrap_or_default();
            for r in children(p) {
                match r.tag_name().name() {
                    "r" | "fld" => {
                        let t: String = child(r, "t").and_then(|t| t.text()).unwrap_or("").to_string();
                        if !t.trim().is_empty() {
                            let props = child(r, "rPr").map(|rp| self.run_props(rp)).unwrap_or_default();
                            sizes.push(props.size_pt);
                            if first_run.is_none() {
                                first_run = Some(props);
                                first_para = Some(para.clone());
                            }
                        }
                        line.push_str(&t);
                    }
                    "br" => line.push('\n'),
                    _ => {}
                }
            }
            lines.push(line);
        }
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        let text = lines.join("\n");
        if text.trim().is_empty() {
            return None;
        }
        sizes.dedup();
        if sizes.len() > 1 {
            self.warn("Gemischte Schriftgrößen in einem Textfeld wurden vereinheitlicht");
        }

        // Vererbungskette: Run → eigener lstStyle → Layout/Master-Platzhalter → Master-Textstile
        let (own_run, own_para) = child(body, "lstStyle").map(|l| self.level1(l)).unwrap_or_default();
        let (style_run, style_para) = match (styles, ph_kind.map(norm_kind)) {
            (Some(s), Some("title")) => s.title.clone(),
            (Some(s), Some(_)) => s.body.clone(),
            (Some(s), None) => s.other.clone(),
            _ => Default::default(),
        };
        let mut run = first_run.unwrap_or_default().or(&own_run);
        let mut para = first_para.unwrap_or_default();
        para.align = para.align.or(own_para.align);
        para.line_height = para.line_height.or(own_para.line_height);
        for i in inherited {
            run = run.or(&i.run);
            para.align = para.align.or(i.para.align);
            para.line_height = para.line_height.or(i.para.line_height);
        }
        run = run.or(&style_run);
        para.align = para.align.or(style_para.align);
        para.line_height = para.line_height.or(style_para.line_height);

        let body_pr = child(body, "bodyPr");
        let align_v = body_pr.and_then(anchor).or_else(|| inherited.iter().find_map(|i| i.anchor)).unwrap_or(VAlign::Top);
        let inset = |k: &str, d: i64| self.px(body_pr.and_then(|b| attr_i64(b, k)).unwrap_or(d) as f32);
        let (l, t, r, b) = (inset("lIns", 91_440), inset("tIns", 45_720), inset("rIns", 91_440), inset("bIns", 45_720));
        let mut frame = Rect::new(frame.x + l, frame.y + t, (frame.w - l - r).max(1.0), (frame.h - t - b).max(1.0));

        let align_h = para.align.unwrap_or(HAlign::Left);
        if body_pr.and_then(|b| attr(b, "wrap")) == Some("none") {
            frame = widen(frame, align_h, self.width as f32);
        }

        let default_pt = match ph_kind.map(norm_kind) {
            Some("title") => 44.0,
            Some(_) => 28.0,
            None => 18.0,
        };
        let (family, weight) = split_weight(run.typeface.as_deref().unwrap_or("Calibri"));
        let weight = if run.bold == Some(true) { weight.max(700) } else { weight };
        if !self.fonts.contains(&family) {
            self.fonts.push(family.clone());
        }

        Some(Element::new(
            name,
            frame,
            ElementKind::Text(TextStyle {
                text,
                font_family: family,
                weight,
                italic: run.italic.unwrap_or(false),
                size_px: self.px(run.size_pt.unwrap_or(default_pt) * 12_700.0),
                color: run.color.or_else(|| self.theme.colors.get("dk1").copied()).unwrap_or(Color::BLACK),
                align_h,
                align_v,
                line_height: para.line_height.unwrap_or(1.2),
                auto_shrink: true,
                shadow: run.shadow,
                outline: run.outline,
            }),
        ))
    }
}

#[derive(Default)]
struct MasterStyles {
    title: (RunProps, ParaProps),
    body: (RunProps, ParaProps),
    other: (RunProps, ParaProps),
}

struct Inheritance<'a> {
    layout: &'a [Inherited],
    master: &'a [Inherited],
    styles: &'a MasterStyles,
}

impl Inheritance<'_> {
    /// Passende Platzhalter aus Layout und Master, wichtigster zuerst.
    fn find(&self, kind: &str, idx: Option<&str>) -> Vec<&Inherited> {
        let kind = norm_kind(kind);
        let same_kind = |p: &&Inherited| norm_kind(&p.kind) == kind;
        let layout =
            idx.and_then(|i| self.layout.iter().find(|p| p.idx.as_deref() == Some(i))).or_else(|| self.layout.iter().find(same_kind));
        layout.into_iter().chain(self.master.iter().find(same_kind)).collect()
    }
}

fn anchor(body_pr: Node) -> Option<VAlign> {
    attr(body_pr, "anchor").map(|a| match a {
        "ctr" => VAlign::Middle,
        "b" => VAlign::Bottom,
        _ => VAlign::Top,
    })
}

fn group_xf(gx: Node, outer: Xf) -> Xf {
    let get = |name: &str, a: &str| child(gx, name).and_then(|n| attr_i64(n, a)).unwrap_or(0) as f32;
    let (off_x, off_y) = (get("off", "x"), get("off", "y"));
    let (ext_x, ext_y) = (get("ext", "cx"), get("ext", "cy"));
    let (ch_x, ch_y) = (get("chOff", "x"), get("chOff", "y"));
    let (che_x, che_y) = (get("chExt", "cx"), get("chExt", "cy"));
    let sx = if che_x > 0.0 { ext_x / che_x } else { 1.0 };
    let sy = if che_y > 0.0 { ext_y / che_y } else { 1.0 };
    Xf {
        ox: outer.ox + outer.sx * (off_x - ch_x * sx),
        oy: outer.oy + outer.sy * (off_y - ch_y * sy),
        sx: outer.sx * sx,
        sy: outer.sy * sy,
    }
}

/// Textfelder ohne Umbruch (`wrap="none"`) passen sich in PowerPoint der Textbreite an.
/// Wir verbreitern sie passend zur Ausrichtung, damit nichts umbricht.
fn widen(f: Rect, align: HAlign, slide_w: f32) -> Rect {
    match align {
        HAlign::Left => Rect::new(f.x, f.y, (slide_w - f.x).max(f.w), f.h),
        HAlign::Right => Rect::new(0.0_f32.min(f.x), f.y, (f.x + f.w).max(f.w), f.h),
        HAlign::Center => {
            let cx = f.x + f.w / 2.0;
            let half = cx.min(slide_w - cx).max(f.w / 2.0);
            Rect::new(cx - half, f.y, half * 2.0, f.h)
        }
    }
}

/// `Roboto Light` → (`Roboto`, 300), damit die Schrift über Familie + Gewicht gefunden wird.
fn split_weight(face: &str) -> (String, u16) {
    const WEIGHTS: [(&str, u16); 9] = [
        (" Thin", 100),
        (" ExtraLight", 200),
        (" Light", 300),
        (" Regular", 400),
        (" Medium", 500),
        (" SemiBold", 600),
        (" Bold", 700),
        (" ExtraBold", 800),
        (" Black", 900),
    ];
    for (suffix, w) in WEIGHTS {
        if let Some(base) = face.strip_suffix(suffix) {
            return (base.to_string(), w);
        }
    }
    (face.to_string(), 400)
}

#[cfg(test)]
mod tests;
