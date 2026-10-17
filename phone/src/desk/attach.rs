use crate::{
    attach,
    chats::{MEDIA_MAX, Media},
    inbox,
    jx::{JNIEnv, JObject, Result},
};
use std::io::Read;
use windows::Win32::{
    Foundation::{HWND, SIZE},
    Graphics::Gdi::{BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC},
    System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree},
    UI::{
        Shell::{Common::COMDLG_FILTERSPEC, FileOpenDialog, IFileOpenDialog, IShellItemImageFactory, SHCreateItemFromParsingName, SIGDN_FILESYSPATH, SIIGBF_RESIZETOFIT},
        WindowsAndMessaging::GetForegroundWindow,
    },
};
use windows::core::{HSTRING, w};

pub fn open(env: &mut JNIEnv, act: &JObject, chat: &str) -> Result<()> {
    inbox::path(env, act)?;
    inbox::update(|s| s.picking_for(chat));
    let (chat, owner) = (chat.to_string(), unsafe { GetForegroundWindow() }.0 as isize);
    std::thread::spawn(move || {
        let Some(path) = (unsafe { pick(HWND(owner as _)) }) else {
            inbox::update(|s| s.picked(&chat));
            return;
        };
        let Some(dir) = inbox::media_dir() else { return };
        attach::set_loading(&chat);
        let mut bytes = Vec::new();
        let read = std::fs::File::open(&path).and_then(|f| f.take(MEDIA_MAX + 1).read_to_end(&mut bytes));
        let mime = mime(&path);
        let r = match (read, Media::kind(mime, &bytes)) {
            (Err(_), _) => Err("Couldn't read that file".to_string()),
            (_, None) => Err(format!("Only photos and videos can be sent ({mime})")),
            (_, Some(k)) if Media::heif(k) => Err("HEIC photos can't be sent from the PC yet".to_string()),
            (_, Some(k)) => attach::keep(&dir, &bytes, k),
        };
        attach::set_result(&chat, r);
    });
    Ok(())
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("heic" | "heif") => "image/heic",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "",
    }
}

unsafe fn pick(owner: HWND) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let types = [COMDLG_FILTERSPEC { pszName: w!("Photos and videos"), pszSpec: w!("*.jpg;*.jpeg;*.png;*.gif;*.webp;*.heic;*.mp4;*.mov;*.webm") }];
        d.SetFileTypes(&types).ok()?;
        d.SetTitle(w!("Send a photo or video")).ok()?;
        d.Show(Some(owner)).ok()?;
        let name = d.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as _));
        path
    }
}

// Shell thumbnails decode photos and video frames at the asked size, so a 12 MP photo never sits in RAM whole.
pub fn picture(path: &str, _: bool, max: i32) -> Option<(slint::Image, f32)> {
    let max = max.min(1800);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let f: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let bmp = f.GetImage(SIZE { cx: max, cy: max }, SIIGBF_RESIZETOFIT).ok()?;
        let mut bm = BITMAP::default();
        GetObjectW(bmp.into(), size_of::<BITMAP>() as i32, Some((&mut bm as *mut BITMAP).cast()));
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, bmp, 0, h as u32, Some(px.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(bmp.into());
        if rows == 0 || w == 0 {
            return None;
        }
        let opaque = px.chunks_exact(4).all(|p| p[3] == 0);
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2);
            if opaque {
                p[3] = 255;
            }
        }
        let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&px, w as u32, h as u32);
        Some((slint::Image::from_rgba8_premultiplied(buf), h as f32 / w as f32))
    }
}

pub fn play(_: &mut JNIEnv, _: &JObject, path: &str) -> Result<()> {
    crate::desk_os::open(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_a_photo_capped_to_the_asked_size() {
        let (w, h) = (64u32, 32u32);
        let row = (w * 3).div_ceil(4) * 4;
        let mut bmp = b"BM".to_vec();
        let size = 54 + row * h;
        for v in [size, 0, 54, 40, w, h] {
            bmp.extend(v.to_le_bytes());
        }
        bmp.extend(1u16.to_le_bytes());
        bmp.extend(24u16.to_le_bytes());
        bmp.extend([0u8; 24]);
        bmp.extend((0..row * h).map(|i| (i % 3 * 100) as u8));
        let path = std::env::temp_dir().join(format!("callme-pic-{}.bmp", std::process::id()));
        std::fs::write(&path, bmp).unwrap();
        let (img, aspect) = super::picture(path.to_str().unwrap(), false, 16).expect("decoded");
        let _ = std::fs::remove_file(path);
        assert!(img.size().width <= 16 && img.size().width > 0);
        assert!((aspect - 0.5).abs() < 0.1);
    }
}
