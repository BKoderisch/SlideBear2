//! Rendert die drei Beispiel-Slides als PNG: `cargo run -p slidebear-render --example render_demo -- <ausgabeordner> [hintergrund...]`

use chrono::NaiveDateTime;
use slidebear_core::scene::{Color, Element, ElementKind, Rect, Shadow, ShapeKind, ShapeStyle};
use slidebear_core::{presets, EventFields, placeholder};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::PathBuf::from(args.first().map(String::as_str).unwrap_or("demo-out"));
    std::fs::create_dir_all(&out)?;
    let backgrounds: Vec<String> = args.iter().skip(1).cloned().collect();

    let dt = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
    let events = [
        ("adventsmarkt", "Adventsmarkt", dt("2024-12-07 15:00"), Some(dt("2024-12-07 19:00")), "", "Fleißige Helfer gesucht!", "%d.%m.%Y"),
        ("herbstputz", "Herbstputz", dt("2024-10-12 10:00"), None, "", "", "%d.%m.%y"),
        ("gemeindeforum", "Gemeindeforum", dt("2025-09-17 19:30"), None, "FeG Adlershof", "", "%d.%m.%Y"),
    ];

    let assets = backgrounds.first().map(|p| std::path::Path::new(p).parent().unwrap().to_path_buf()).unwrap_or(out.clone());
    let mut renderer = slidebear_render::Renderer::new(assets);

    for (i, (slug, title, start, end, ort, sub, date)) in events.into_iter().enumerate() {
        let bg = backgrounds.get(i).map(|p| std::path::Path::new(p).file_name().unwrap().to_string_lossy().to_string());
        let mut tpl = presets::event_template(slug, bg.as_deref());
        tpl.date_style.pattern = date.into();
        if bg.is_none() {
            // Ersatz-Hintergrund: dunkler Grund mit ein paar warmen Lichtpunkten
            let mut els = vec![Element::new("bg", Rect::new(0.0, 0.0, 1920.0, 1080.0), ElementKind::Shape(ShapeStyle { shape: ShapeKind::Rect, fill: Some(Color::rgb(28, 26, 24)), stroke: None }))];
            for k in 0..14 {
                let x = (k * 173 % 1800) as f32;
                let y = (k * 311 % 1000) as f32;
                els.push(Element::new("dot", Rect::new(x, y, 70.0, 70.0), ElementKind::Shape(ShapeStyle { shape: ShapeKind::Ellipse, fill: Some(Color::rgba(240, 160, 40, 140)), stroke: None })));
            }
            els.append(&mut tpl.scene.elements);
            tpl.scene.elements = els;
        }
        if let ElementKind::Text(t) = &mut tpl.scene.elements.iter_mut().rev().nth(2).unwrap().kind {
            t.shadow = Some(Shadow { color: Color::rgba(0, 0, 0, 160), offset_x: 0.0, offset_y: 4.0, blur: 8.0 });
        }
        let fields = EventFields { title: title.into(), start, end, all_day: false, location: ort.into(), subtitle: sub.into() };
        let t0 = std::time::Instant::now();
        let pm = renderer.render(&tpl.scene, &|s| placeholder::resolve(s, &fields, tpl.as_slide_ref()));
        let path = out.join(format!("{slug}.png"));
        slidebear_render::save_png(&pm, &path)?;
        println!("{} ({} ms)", path.display(), t0.elapsed().as_millis());
    }
    Ok(())
}
