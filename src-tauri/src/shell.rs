//! Open/reveal/properties/elevate — ports FlexExplorer's
//! `src-tauri/src/shell.rs` verbatim for the shared parts, plus new
//! elevation commands FlexFind needs that FlexExplorer doesn't.

/// Open a path with its default app via `ShellExecuteExW`'s "open" verb.
///
/// Deliberately NOT `cmd /C start "" <path>` (FlexExplorer's original
/// pattern, and this app's own v1): `std::process::Command` on Windows only
/// quotes arguments containing whitespace, so a path with a cmd.exe
/// metacharacter (`&`, `^`, `%`, `(`, `)` — all legal in filenames) reaches
/// cmd unquoted and gets interpreted as a command separator/operator. That's
/// both a correctness bug (Enter on such a file does the wrong thing) and,
/// for a launcher that indexes attacker-nameable files, a real command-
/// injection primitive. `ShellExecuteExW` takes the path as data, never as
/// a shell command line, so it's immune to this class of issue and also
/// handles folders (opens them in Explorer) the same as before.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::shell_open(&path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("unsupported".into())
    }
}

/// Launch an executable directly (no shell involved, so no quoting/
/// metacharacter concerns) with the given argv. Used for handing off to the
/// FlexExplorer/FlexGrep sibling apps with a target path — plain
/// `open_path` can't pass extra arguments since it goes through a single
/// shell-open verb.
#[tauri::command]
pub fn launch_app(exe: String, args: Vec<String>) -> Result<(), String> {
    std::process::Command::new(exe)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Duplicate a file into the same folder as
/// `<stem>_<YYYYMMDD>_<NN><ext>`, where `NN` is the smallest 2-digit
/// counter (01, 02, …) that doesn't collide with an existing file — reusing
/// this on the same day just keeps incrementing instead of overwriting the
/// previous dated copy. Folders are rejected: a recursive folder copy is a
/// different, heavier operation than this "quick dated snapshot" is for.
#[tauri::command]
pub fn duplicate_as_dated_copy(path: String) -> Result<String, String> {
    let src = std::path::Path::new(&path);
    if !src.is_file() {
        return Err("フォルダは複製できません".into());
    }
    let dir = src.parent().ok_or("親フォルダを取得できません")?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = src.extension().and_then(|s| s.to_str());
    let date = chrono::Local::now().format("%Y%m%d").to_string();

    for n in 1..=99u32 {
        let candidate_name = match ext {
            Some(e) => format!("{stem}_{date}_{n:02}.{e}"),
            None => format!("{stem}_{date}_{n:02}"),
        };
        let candidate = dir.join(&candidate_name);
        if !candidate.exists() {
            std::fs::copy(src, &candidate).map_err(|e| e.to_string())?;
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    Err("同名候補が上限(99件/日)に達しました".into())
}

/// Reveal a path in Explorer with it selected.
#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("unsupported".into())
    }
}

/// Invoke a shell verb on a path (used for "プロパティ" -> verb "properties").
#[tauri::command]
pub fn shell_verb(path: String, verb: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::shell_verb(&path, &verb)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, verb);
        Err("shell verbs are only available on Windows".into())
    }
}

/// Relaunch the current executable elevated (UAC "runas"), then exit this
/// unelevated instance. `async` so the main thread isn't blocked while the
/// UAC consent dialog is up.
#[tauri::command]
pub async fn elevate_restart(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::elevate_restart(app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("elevation is only available on Windows".into())
    }
}

/// Whether the current process is running with administrator rights. Used
/// only to drive the settings window's warning banner / status text — not
/// a security check, so `IsUserAnAdmin` (a simple UI-hint-grade query) is
/// sufficient; a full access-token inspection would be overkill here.
#[tauri::command]
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        win::is_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod win {
    // Leading `::` forces resolution to the extern `windows` crate rather
    // than this crate's own `crate::windows` (window show/hide/position)
    // module — both are named `windows`, which is otherwise ambiguous.
    use ::windows::core::PCWSTR;
    use ::windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteExW, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW};
    use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Plain "open" verb, no `SEE_MASK_INVOKEIDLIST` — that flag is what
    /// makes `shell_verb` able to invoke arbitrary (possibly non-standard,
    /// e.g. "properties") verbs, but it also makes failures/edge cases
    /// noisier than the simple default-open path needs.
    pub fn shell_open(path: &str) -> Result<(), String> {
        let wpath = wide(path);
        let wverb = wide("open");
        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.lpVerb = PCWSTR(wverb.as_ptr());
        info.lpFile = PCWSTR(wpath.as_ptr());
        info.nShow = SW_SHOWNORMAL.0;
        unsafe { ShellExecuteExW(&mut info).map_err(|e| e.to_string()) }
    }

    pub fn shell_verb(path: &str, verb: &str) -> Result<(), String> {
        let wpath = wide(path);
        let wverb = wide(verb);
        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_INVOKEIDLIST;
        info.lpVerb = PCWSTR(wverb.as_ptr());
        info.lpFile = PCWSTR(wpath.as_ptr());
        info.nShow = SW_SHOWNORMAL.0;
        unsafe { ShellExecuteExW(&mut info).map_err(|e| e.to_string()) }
    }

    pub fn elevate_restart(app: tauri::AppHandle) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let wexe = wide(&exe.to_string_lossy());
        let wverb = wide("runas");
        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.lpVerb = PCWSTR(wverb.as_ptr());
        info.lpFile = PCWSTR(wexe.as_ptr());
        info.nShow = SW_SHOWNORMAL.0;
        unsafe { ShellExecuteExW(&mut info).map_err(|e| e.to_string())? };
        app.exit(0);
        Ok(())
    }

    pub fn is_elevated() -> bool {
        unsafe { IsUserAnAdmin().as_bool() }
    }
}
