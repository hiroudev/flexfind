//! Extracts the real Windows shell icon for a file type / folder and returns it
//! as a PNG data URL. Uses SHGetFileInfo with SHGFI_USEFILEATTRIBUTES so the
//! icon is resolved from the name's extension without touching the disk — fast
//! and cacheable by extension on the frontend.

use base64::Engine;

#[tauri::command]
pub fn shell_icon(name: String, folder: bool, large: bool) -> Result<String, String> {
    #[cfg(windows)]
    {
        let png = win::shell_icon_png(&name, folder, large)?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(format!("data:image/png;base64,{b64}"))
    }
    #[cfg(not(windows))]
    {
        let _ = (name, folder, large);
        Err("shell icons are only available on Windows".into())
    }
}

/// Like `shell_icon` but resolves the icon from a real path — so drives,
/// special folders (Desktop/Downloads), and apps get their true icons.
#[tauri::command]
pub fn shell_icon_for_path(path: String, large: bool) -> Result<String, String> {
    #[cfg(windows)]
    {
        let png = win::shell_icon_png_for_path(&path, large)?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(format!("data:image/png;base64,{b64}"))
    }
    #[cfg(not(windows))]
    {
        let _ = (path, large);
        Err("shell icons are only available on Windows".into())
    }
}

#[cfg(windows)]
mod win {
    use core::ffi::c_void;
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    };
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SMALLICON,
        SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    pub fn shell_icon_png(name: &str, folder: bool, large: bool) -> Result<Vec<u8>, String> {
        let attr = if folder { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
        let size_flag = if large { SHGFI_LARGEICON } else { SHGFI_SMALLICON };
        icon_png(name, attr, SHGFI_ICON | SHGFI_USEFILEATTRIBUTES | size_flag, large)
    }

    pub fn shell_icon_png_for_path(path: &str, large: bool) -> Result<Vec<u8>, String> {
        let size_flag = if large { SHGFI_LARGEICON } else { SHGFI_SMALLICON };
        // No USEFILEATTRIBUTES → resolve the real item's icon (drive/special folder).
        icon_png(path, FILE_ATTRIBUTE_NORMAL, SHGFI_ICON | size_flag, large)
    }

    fn icon_png(
        target: &str,
        attr: windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        flags: windows::Win32::UI::Shell::SHGFI_FLAGS,
        _large: bool,
    ) -> Result<Vec<u8>, String> {
        unsafe {
            let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
            let mut shfi = SHFILEINFOW::default();
            let r = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                attr,
                Some(&mut shfi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags,
            );
            if r == 0 || shfi.hIcon.is_invalid() {
                return Err("SHGetFileInfo returned no icon".into());
            }
            let result = hicon_to_png(shfi.hIcon);
            let _ = DestroyIcon(shfi.hIcon);
            result
        }
    }

    unsafe fn hicon_to_png(hicon: HICON) -> Result<Vec<u8>, String> {
        let mut ii = ICONINFO::default();
        GetIconInfo(hicon, &mut ii).map_err(|e| e.to_string())?;

        let mut bmp = BITMAP::default();
        let got = GetObjectW(
            HGDIOBJ(ii.hbmColor.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut c_void),
        );
        if got == 0 {
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
            return Err("GetObject(color bitmap) failed".into());
        }
        let w = bmp.bmWidth;
        let h = bmp.bmHeight;
        if w <= 0 || h <= 0 {
            let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
            return Err("icon bitmap has no size".into());
        }

        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = 0; // BI_RGB

        let mut buf = vec![0u8; (w * h * 4) as usize];
        let hdc = GetDC(None);
        let scanned = GetDIBits(
            hdc,
            ii.hbmColor,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut c_void),
            &mut bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, hdc);
        let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
        if scanned == 0 {
            return Err("GetDIBits failed".into());
        }

        // Convert BGRA -> RGBA; recover alpha from a flat icon if necessary.
        let mut any_alpha = false;
        let mut rgba = vec![0u8; buf.len()];
        for i in (0..buf.len()).step_by(4) {
            let b = buf[i];
            let g = buf[i + 1];
            let r = buf[i + 2];
            let a = buf[i + 3];
            if a != 0 {
                any_alpha = true;
            }
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = a;
        }
        if !any_alpha {
            for i in (0..rgba.len()).step_by(4) {
                rgba[i + 3] = 255;
            }
        }

        encode_png(w as u32, h as u32, &rgba)
    }

    fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
        use image::ImageEncoder;
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        Ok(out)
    }
}
