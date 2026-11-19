//! Teste la capture de chaque fenêtre ouverte, hors de l'app.
//! `cargo run --example capture_probe [filtre]`
fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    for w in oz_lib::windows_api::enumerate_windows() {
        if !filter.is_empty() && !w.title.to_lowercase().contains(&filter) && !w.process.to_lowercase().contains(&filter) {
            continue;
        }
        match oz_lib::windows_api::capture_window_jpeg(w.raw()) {
            Ok(png) => {
                if let Ok(dir) = std::env::var("CAPTURE_DIR") { let _ = std::fs::write(format!("{dir}/{}.jpg", w.process), &png); }
                println!("OK   {} ko  {} — {}", png.len() / 1024, w.process, w.title)
            }
            Err(e) => println!("ERR  {e}  {} — {}", w.process, w.title),
        }
    }
}
