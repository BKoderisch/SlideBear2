//! PPTX importieren und alle Folien rendern: `cargo run -p slidebear-render --example import_render -- datei.pptx ausgabe/`

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::PathBuf::from(&args[1]);
    let assets = out.join("assets");
    std::fs::create_dir_all(&assets)?;
    let imp = slidebear_pptx::import_file(std::path::Path::new(&args[0]))?;
    for (name, data) in &imp.assets {
        std::fs::write(assets.join(name), data)?;
    }
    println!("Schriften: {:?}", imp.fonts);
    let mut r = slidebear_render::Renderer::new(&assets);
    for s in &imp.slides {
        let pm = r.render(&s.scene, &|t| t.to_string());
        let p = out.join(format!("folie{}.png", s.number));
        slidebear_render::save_png(&pm, &p)?;
        println!("{} ({} Elemente) {:?}", p.display(), s.scene.elements.len(), s.warnings);
        let d = slidebear_core::smart::detect(&s.scene);
        for rep in &d.replacements {
            println!("   {:?} -> {:?}", rep.before, rep.after);
        }
    }
    Ok(())
}
