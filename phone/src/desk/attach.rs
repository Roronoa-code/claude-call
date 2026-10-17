use crate::{
    attach,
    chats::{MEDIA_MAX, Media},
    inbox,
    jx::{JNIEnv, JObject, Result},
};
use std::{io::Read, path::{Path, PathBuf}};
use windows::Win32::{
    Foundation::{HWND, SIZE},
    Graphics::Gdi::{BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC},
    System::{Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree}, DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW}},
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
        attach::set_result(&chat, read_file(&dir, Path::new(&path)));
    });
    Ok(())
}

fn read_file(dir: &Path, path: &Path) -> std::result::Result<Media, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).and_then(|f| f.take(MEDIA_MAX + 1).read_to_end(&mut bytes)).map_err(|_| "Couldn't read that file".to_string())?;
    let mime = mime(&path.to_string_lossy());
    match Media::kind(mime, &bytes) {
        None => Err("Only photos and videos can be sent".into()),
        Some(k) if Media::heif(k) => {
            if bytes.len() as u64 > MEDIA_MAX { return Err("That file is over 10 MB".into()) }
            attach::keep(dir, &jpeg(&bytes)?, "image/jpeg")
        }
        Some(k) => attach::keep(dir, &bytes, k),
    }
}

fn jpeg(bytes: &[u8]) -> std::result::Result<Vec<u8>, String> {
    use windows::{Graphics::Imaging::{BitmapDecoder, BitmapTransform, BitmapPixelFormat, BitmapAlphaMode, ExifOrientationMode, ColorManagementMode}, Storage::Streams::{DataWriter, InMemoryRandomAccessStream}};
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoUninitialize};
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().map_err(|_| "Couldn't open the photo decoder".to_string())?;
    let decoded = (|| -> windows::core::Result<_> {
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(bytes)?;
        writer.StoreAsync()?.join()?;
        stream.Seek(0)?;
        let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
        let (w, h) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
        if w == 0 || h == 0 { return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG)) }
        let mut sample = 1;
        while w.max(h) / sample > 4096 { sample *= 2; }
        let (mut w, mut h) = ((w / sample).max(1), (h / sample).max(1));
        let transform = BitmapTransform::new()?;
        transform.SetScaledWidth(w)?;
        transform.SetScaledHeight(h)?;
        let pixels = decoder.GetPixelDataTransformedAsync(BitmapPixelFormat::Rgba8, BitmapAlphaMode::Straight, &transform,
            ExifOrientationMode::RespectExifOrientation, ColorManagementMode::ColorManageToSRgb)?.join()?.DetachPixelData()?;
        if decoder.OrientedPixelWidth()? != decoder.PixelWidth()? { std::mem::swap(&mut w, &mut h); }
        Ok((w, h, pixels))
    })();
    unsafe { CoUninitialize() };
    let (w, h, pixels) = decoded.map_err(|_| "Couldn't decode that photo. HEIC needs Windows HEIF and HEVC image support".to_string())?;
    let rgba = image::RgbaImage::from_raw(w, h, pixels.to_vec()).ok_or_else(|| "Couldn't read that photo".to_string())?;
    let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 90).encode_image(&rgb).map_err(|_| "Couldn't convert that photo".to_string())?;
    Ok(output)
}

fn receive(chat: &str, read: impl FnOnce(&Path) -> std::result::Result<Media, String> + Send + 'static) {
    if inbox::update(|s| {
        if !s.picking.is_empty() { return false }
        s.picking_for(chat);
        true
    }) != Some(true) { return }
    attach::set_loading(chat);
    let chat = chat.to_string();
    std::thread::spawn(move || {
        let result = inbox::media_dir().ok_or_else(|| "Couldn't save that file".to_string()).and_then(|dir| read(&dir));
        attach::set_result(&chat, result);
    });
}

pub fn dropped(chat: &str, path: PathBuf) {
    receive(chat, move |dir| read_file(dir, &path));
}

fn png(image: arboard::ImageData<'_>) -> std::result::Result<Vec<u8>, String> {
    let pixels = image.width.checked_mul(image.height).filter(|&n| n > 0 && n <= 4096 * 4096)
        .ok_or_else(|| "That image is too large to paste".to_string())?;
    if image.bytes.len() != pixels * 4 {
        return Err("Couldn't read that image".into());
    }
    let mut bytes = Vec::new();
    use image::ImageEncoder;
    image::codecs::png::PngEncoder::new(&mut bytes).write_image(&image.bytes, image.width as u32, image.height as u32, image::ExtendedColorType::Rgba8)
        .map_err(|_| "Couldn't read that image".to_string())?;
    Ok(bytes)
}

pub fn paste(chat: &str) -> bool {
    let files = unsafe { IsClipboardFormatAvailable(15).is_ok() };
    let image = unsafe { IsClipboardFormatAvailable(17).is_ok() || IsClipboardFormatAvailable(RegisterClipboardFormatW(w!("PNG"))).is_ok() };
    if !files && !image { return false }
    receive(chat, move |dir| {
        let mut clipboard = arboard::Clipboard::new().map_err(|_| "Couldn't open the clipboard".to_string())?;
        if files {
            let paths = clipboard.get().file_list().map_err(|_| "Couldn't read that file".to_string())?;
            if paths.len() != 1 { return Err("Attach one photo or video at a time".into()) }
            read_file(dir, &paths[0])
        } else {
            let image = clipboard.get_image().map_err(|_| "Couldn't read that image".to_string())?;
            attach::keep(dir, &png(image)?, "image/png")
        }
    });
    true
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
        let types = [COMDLG_FILTERSPEC { pszName: w!("Photos and videos"), pszSpec: w!("*.jpg;*.jpeg;*.png;*.gif;*.webp;*.heic;*.heif;*.mp4;*.mov;*.webm") }];
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
    fn photo_conversion_scales_and_respects_orientation() {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(&vec![80; 5000 * 2 * 3], 5000, 2, image::ExtendedColorType::Rgb8).unwrap();
        let jpg = super::jpeg(&png).unwrap();
        let decoded = image::load_from_memory(&jpg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2500, 1));
        // EXIF orientation 6 rotates the wide image clockwise.
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut rotated = jpg[..2].to_vec();
        rotated.extend([0xff, 0xe1]);
        rotated.extend(((exif.len() + 2) as u16).to_be_bytes());
        rotated.extend(exif);
        rotated.extend(&jpg[2..]);
        let decoded = image::load_from_memory(&super::jpeg(&rotated).unwrap()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1, 2500));
        assert!(super::jpeg(b"broken photo").is_err());
    }

    #[test]
    #[ignore = "requires CALLME_HEIC_FIXTURE and Windows HEIF/HEVC codecs"]
    fn installed_heic_codec_converts_a_real_photo() {
        let path = std::path::PathBuf::from(std::env::var("CALLME_HEIC_FIXTURE").expect("fixture path"));
        let bytes = std::fs::read(&path).unwrap();
        assert!(crate::chats::Media::kind("", &bytes).is_some_and(crate::chats::Media::heif));
        let dir = std::env::temp_dir().join(format!("callme-heic-{}-{}", std::process::id(), crate::jx::now_ms()));
        let media = super::read_file(&dir, &path).unwrap();
        assert_eq!(media.mime, "image/jpeg");
        let decoded = image::load_from_memory(&std::fs::read(&media.path).unwrap()).unwrap();
        assert!(decoded.width() > 0 && decoded.height() > 0);
        assert!(decoded.width().max(decoded.height()) <= 4096);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pasted_pixels_keep_color_and_alpha_and_reject_invalid_sizes() {
        let pixels = [20, 40, 80, 128];
        let bytes = super::png(arboard::ImageData { width: 1, height: 1, bytes: std::borrow::Cow::Borrowed(&pixels) }).unwrap();
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap().to_rgba8();
        assert_eq!(decoded.as_raw(), &pixels);
        for (width, height, bytes) in [(1, 1, &pixels[..3]), (0, 1, &pixels[..]), (usize::MAX, 2, &pixels[..]), (4097, 4096, &pixels[..])] {
            assert!(super::png(arboard::ImageData { width, height, bytes: std::borrow::Cow::Borrowed(bytes) }).is_err());
        }
    }

    #[test]
    fn dropped_files_use_shared_media_validation() {
        let dir = std::env::temp_dir().join(format!("callme-drop-{}-{}", std::process::id(), crate::jx::now_ms()));
        std::fs::create_dir(&dir).unwrap();
        let input = dir.join("photo.PNG");
        let bytes = include_bytes!("../../ui/icon/claude-call.png");
        std::fs::write(&input, bytes).unwrap();
        let media = super::read_file(&dir, &input).unwrap();
        assert_eq!(std::fs::read(&media.path).unwrap(), bytes);
        assert_eq!(media.mime, "image/png");
        let oversized = std::fs::OpenOptions::new().write(true).open(&input).unwrap();
        oversized.set_len(crate::chats::MEDIA_MAX + 1).unwrap();
        drop(oversized);
        assert!(super::read_file(&dir, &input).unwrap_err().contains("over 10 MB"));
        let unsupported = dir.join("document.txt");
        std::fs::write(&unsupported, b"not a photo").unwrap();
        assert!(super::read_file(&dir, &unsupported).is_err());
        assert!(super::read_file(&dir, &dir.join("missing.jpg")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
