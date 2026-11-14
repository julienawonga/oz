fn main() {
    let windows = app_lib::enumerate_windows();
    println!("=== Titres bruts vus par le robot ===");
    for w in &windows {
        println!("  [{}] {}", w.process, w.title);
    }
    println!("\n=== Sortie formatée (ce que dit Mini-Julien) ===");
    let mut seen = std::collections::HashSet::new();
    for w in &windows {
        let app = app_lib::friendly_name(&w.process);
        if seen.insert(app.clone()) {
            println!("• {} — {}", app, app_lib::clean_title(&w.title));
        }
    }
}
