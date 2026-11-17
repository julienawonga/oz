fn main() {
    let windows = oz_lib::windows_api::enumerate_windows();
    println!("=== Titres bruts vus par le robot ===");
    for w in &windows {
        println!("  [{}] {}", w.process, w.title);
    }
    println!("\n=== Sortie formatée (ce que dit Mini-Julien) ===");
    let mut seen = std::collections::HashSet::new();
    for w in &windows {
        let app = oz_lib::windows_api::friendly_name(&w.process);
        if seen.insert(app.clone()) {
            println!("• {} — {}", app, oz_lib::windows_api::clean_title(&w.title));
        }
    }
}
