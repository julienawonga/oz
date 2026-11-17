//! Accès Win32 : énumération des fenêtres, capture, actions sur fenêtre.
//! Tout le code `unsafe` lié à l'OS vit ici.

use serde::Serialize;
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
    DIB_RGB_COLORS, GetDC, ReleaseDC, SelectObject,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SetForegroundWindow, ShowWindow,
    SW_RESTORE, WM_CLOSE,
};

const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(2u32);

#[derive(Serialize, Clone)]
pub struct AppWindow {
    pub(crate) title: String,
    pub(crate) process: String,
}

pub(crate) unsafe fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

pub(crate) unsafe fn process_image_name(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return "?".into();
        };
        let mut buf = vec![0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size);
        if ok.is_err() {
            return "?".into();
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        full.rsplit(['\\', '/']).next().unwrap_or("?").to_string()
    }
}

unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let list = &mut *(lparam.0 as *mut Vec<WindowHandle>);
        if IsWindowVisible(hwnd).as_bool() {
            let title = window_title(hwnd);
            if !title.is_empty() {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let process = process_image_name(pid);
                // On ne s'énumère pas soi-même
                if process != "oz.exe" && process != "Oz.exe" {
                    list.push(WindowHandle {
                        hwnd: hwnd.0 as isize,
                        title,
                        process,
                    });
                }
            }
        }
        true.into()
    }
}

#[derive(Clone)]
pub struct WindowHandle {
    pub(crate) hwnd: isize,
    pub title: String,
    pub process: String,
}

impl WindowHandle {
    pub fn key(&self) -> String {
        format!("{}::{}", self.process, self.title)
    }
}

pub fn enumerate_windows() -> Vec<WindowHandle> {
    let mut list: Vec<WindowHandle> = Vec::new();
    unsafe {
        let rc = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut list as *mut Vec<WindowHandle> as isize),
        );
        if rc.is_err() {
            return Vec::new();
        }
    }
    list.sort_by(|a, b| (&a.process, &a.title).cmp(&(&b.process, &b.title)));
    list.dedup_by(|a, b| a.process == b.process && a.title == b.title);
    // Filtre les hôtes système invisibles pour l'utilisateur (bruit Windows)
    const NOISE: &[&str] = &[
        "applicationframehost", "textinputhost", "searchhost",
        "startmenuexperiencehost", "shellexperiencehost", "msedgewebview2",
        "runtimebroker", "svchost", "conhost", "dllhost",
    ];
    list.retain(|w| {
        let base = w.process.to_lowercase().trim_end_matches(".exe").to_string();
        if NOISE.contains(&base.as_str()) {
            return false;
        }
        // "Program Manager" = la pseudo-fenêtre du bureau (explorer)
        if w.title == "Program Manager" {
            return false;
        }
        true
    });
    list
}

/// Nom d'application lisible (processus Windows → nom humain).
pub fn friendly_name(process: &str) -> String {
    let base = process.to_lowercase().trim_end_matches(".exe").to_string();
    let name = match base.as_str() {
        "msedge" => "Edge",
        "chrome" => "Chrome",
        "firefox" => "Firefox",
        "cmd" | "windowsterminal" | "wterminal" => "Terminal",
        "powershell" | "pwsh" => "PowerShell",
        "code" => "VS Code",
        "notepad" | "notepad++" => "Bloc-notes",
        "explorer" => "Explorateur",
        "onvue" => "OnVUE",
        "spotify" => "Spotify",
        "discord" => "Discord",
        "ollama" => "Ollama",
        "telegram" => "Telegram",
        "steam" => "Steam",
        "obs64" => "OBS",
        "systemsettings" => "Paramètres",
        "taskmgr" => "Gestionnaire des tâches",
        "vlc" => "VLC",
        "word" | "winword" => "Word",
        "excel" => "Excel",
        "outlook" => "Outlook",
        _ => {
            // Capitalise le premier caractère par défaut
            let mut c = base.chars();
            match c.next() {
                Some(f) => return f.to_uppercase().collect::<String>() + c.as_str(),
                None => return "App".into(),
            }
        }
    };
    name.to_string()
}

/// Nettoie un titre de fenêtre : retire les suffixes de navigateur connus,
/// puis raccourcit si trop long.
pub fn clean_title(title: &str) -> String {
    let mut t = title.to_string();
    for suffix in [
        " - Personnel - Microsoft Edge",
        " - Microsoft Edge",
        " - Google Chrome",
        " — Mozilla Firefox",
        " - Mozilla Firefox",
        " - Bloc-notes",
        " - Visual Studio Code",
    ] {
        if let Some(cleaned) = t.strip_suffix(suffix) {
            t = cleaned.to_string();
            break;
        }
    }
    // Un titre qui EST un chemin : ne garde que le dernier segment
    if t.contains('\\') {
        t = t
            .rsplit('\\')
            .next()
            .unwrap_or(&t)
            .to_string();
    }
    // Suffixes de navigateur — gère les tirets unicode et caractères invisibles
    for browser in ["Microsoft Edge", "Google Chrome", "Mozilla Firefox"] {
        if let Some(pos) = t.rfind(browser) {
            let before = &t[..pos];
            t = before
                .trim_end_matches(|c: char| {
                    matches!(c, '-' | '–' | '—' | ' ' | '\u{200b}' | '\u{200e}')
                })
                .to_string();
            break;
        }
    }
    if t.chars().count() > 45 {
        t = t.chars().take(42).collect::<String>() + "…";
    }
    t
}

#[tauri::command]
pub(crate) fn list_windows() -> Result<Vec<AppWindow>, String> {
    Ok(enumerate_windows()
        .into_iter()
        .map(|w| AppWindow {
            title: w.title,
            process: w.process,
        })
        .collect())
}

pub(crate) fn capture_window_png(hwnd: HWND) -> Result<Vec<u8>, String> {
    unsafe {
        let mut rc = windows::Win32::Foundation::RECT::default();
        GetWindowRect(hwnd, &mut rc).map_err(|e| e.to_string())?;
        let w = (rc.right - rc.left).max(1);
        let h = (rc.bottom - rc.top).max(1);

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbm = CreateDIBSection(
            Some(hdc_mem),
            &bi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
        .map_err(|e| e.to_string())?;
        let old = SelectObject(hdc_mem, hbm.into());
        let ok = PrintWindow(hwnd, hdc_mem, PW_RENDERFULLCONTENT).as_bool();

        // BGRA → RGBA
        let len = (w * h * 4) as usize;
        let slice = std::slice::from_raw_parts(bits as *const u8, len);
        let mut rgba = Vec::with_capacity(len);
        for px in slice.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }

        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbm.into());
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        if !ok {
            return Err("PrintWindow a échoué".into());
        }
        let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
            .ok_or("image invalide")?;
        let mut png: Vec<u8> = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(png)
    }
}

/// Handle de la fenêtre actuellement au premier plan (0 si aucune).
pub(crate) fn foreground_hwnd() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

impl WindowHandle {
    fn raw(&self) -> HWND {
        HWND(self.hwnd as *mut core::ffi::c_void)
    }

    /// Demande la fermeture de la fenêtre (WM_CLOSE).
    pub(crate) fn close(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.raw()), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    /// Restaure la fenêtre et la met au premier plan.
    pub(crate) fn focus(&self) {
        unsafe {
            let _ = ShowWindow(self.raw(), SW_RESTORE);
            let _ = SetForegroundWindow(self.raw());
        }
    }

    /// Capture PNG de la fenêtre.
    pub(crate) fn capture_png(&self) -> Result<Vec<u8>, String> {
        capture_window_png(self.raw())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_name_maps_known_processes() {
        assert_eq!(friendly_name("chrome.exe"), "Chrome");
        assert_eq!(friendly_name("WINWORD.EXE"), "Word");
        assert_eq!(friendly_name("Code.exe"), "VS Code");
    }

    #[test]
    fn friendly_name_capitalizes_unknown_and_handles_empty() {
        assert_eq!(friendly_name("foobar.exe"), "Foobar");
        assert_eq!(friendly_name(""), "App");
    }

    #[test]
    fn clean_title_strips_browser_suffixes() {
        assert_eq!(clean_title("Docs - Google Chrome"), "Docs");
        assert_eq!(clean_title("Titre - Personnel - Microsoft Edge"), "Titre");
        assert_eq!(clean_title("Page — Mozilla Firefox"), "Page");
    }

    #[test]
    fn clean_title_keeps_last_path_segment() {
        assert_eq!(clean_title(r"C:\dossier\sous\fichier.txt"), "fichier.txt");
    }

    #[test]
    fn clean_title_truncates_long_titles() {
        let long = "x".repeat(80);
        let out = clean_title(&long);
        assert_eq!(out.chars().count(), 43);
        assert!(out.ends_with('…'));
        assert_eq!(clean_title("court"), "court");
    }
}
