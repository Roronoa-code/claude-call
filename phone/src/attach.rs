//! A photo or video to send in a chat: Samsung's Gallery or the system picker, reading what was picked into
//! filesDir/media (where the PC's files go too, so it shows and plays the same way), and the chip above the field until
//! it is sent or removed.
//! A pick belongs to the chat it was picked in: once ready it is kept with that chat in the store (chats.json), so it
//! never goes with a message in another chat and stays when the app is closed. The chat the picker is open for is kept
//! in the store too (Store::picking): Samsung may close the app while the Gallery is open, and the pick still lands.
//! The Gallery is tried first; if what it gives back can't be read (no grant, no type), the system picker opens at once
//! instead, and is used from then on until the app is closed.
use crate::{
    Chats, HomeScreen, android,
    chats::{MEDIA_MAX, Media},
    inbox,
    jx::{self, s},
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JByteArray, JObject, JValue},
};
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

/// A pick on its way, or one that can't be sent (why), and the chat it was picked in. Ready ones are in the store.
#[derive(Clone)]
enum Pick {
    Loading,
    Failed(String),
}
static PICK: Mutex<Option<(String, Pick)>> = Mutex::new(None);
/// The chat the picker was opened for (its answer comes later, in picked()).
static FOR: Mutex<String> = Mutex::new(String::new());
const REQUEST: i32 = 7; // MainActivity.onActivityResult in Shell.kt

/// Samsung's own Gallery app: the pick opens there when the phone has it.
const GALLERY: &str = "com.sec.android.gallery3d";
/// The picker open now is the Gallery.
static IN_GALLERY: AtomicBool = AtomicBool::new(false);
/// What the Gallery gave back couldn't be read: the system picker from now on, until the app is closed.
static GALLERY_FAILED: AtomicBool = AtomicBool::new(false);

/// Opens Samsung's Gallery for `chat` (photos and videos), or, without it (or once it failed), the system picker.
pub fn open(env: &mut JNIEnv, act: &JObject, chat: &str) -> Result<()> {
    *FOR.lock().unwrap() = chat.to_string();
    inbox::path(env, act)?;
    inbox::update(|s| s.picking_for(chat));
    let gallery = if GALLERY_FAILED.load(Relaxed) { None } else { gallery(env, act)? };
    IN_GALLERY.store(gallery.is_some(), Relaxed);
    let i = match gallery {
        Some(i) => i,
        None => system(env)?,
    };
    env.call_method(act, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&i), JValue::Int(REQUEST)])?;
    Ok(())
}

/// The system photo picker (photos and videos); Android 10-12 get "choose a file" limited to them.
fn system<'a>(env: &mut JNIEnv<'a>) -> Result<JObject<'a>> {
    let pick_images = jx::sdk(env)? >= 33;
    let i = new_intent(env, if pick_images { "android.provider.action.PICK_IMAGES" } else { "android.intent.action.GET_CONTENT" })?;
    if !pick_images {
        let openable = s(env, "android.intent.category.OPENABLE")?;
        env.call_method(&i, "addCategory", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&openable)])?;
        media_types(env, &i)?;
    }
    Ok(i)
}

/// The Gallery's pick couldn't be read: the system picker opens for the same chat straight away (no question asked),
/// and is used from now on. Whatever was showing "Getting it ready…" goes.
fn instead(env: &mut JNIEnv, act: &JObject, chat: &str) -> Result<()> {
    android::log("attach: the Gallery's pick couldn't be read; the system picker instead");
    GALLERY_FAILED.store(true, Relaxed);
    {
        let mut p = PICK.lock().unwrap();
        if p.as_ref().is_some_and(|(c, _)| c == chat) {
            *p = None;
        }
    }
    inbox::changed();
    *FOR.lock().unwrap() = chat.to_string();
    let i = system(env)?;
    env.call_method(act, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&i), JValue::Int(REQUEST)])?;
    Ok(())
}

/// The Gallery's pick, if the phone has Samsung's Gallery (its PICK takes image and video types: "*/*" with the
/// two lets it offer both). Its answer is read like the system picker's (picked).
fn gallery<'a>(env: &mut JNIEnv<'a>, act: &JObject) -> Result<Option<JObject<'a>>> {
    let i = new_intent(env, "android.intent.action.PICK")?;
    media_types(env, &i)?;
    let pkg = s(env, GALLERY)?;
    env.call_method(&i, "setPackage", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&pkg)])?;
    let pm = env.call_method(act, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])?.l()?;
    let found = env.call_method(&i, "resolveActivity", "(Landroid/content/pm/PackageManager;)Landroid/content/ComponentName;", &[JValue::Object(&pm)])?.l()?;
    Ok((!found.is_null()).then_some(i))
}

fn new_intent<'a>(env: &mut JNIEnv<'a>, action: &str) -> Result<JObject<'a>> {
    let action = s(env, action)?;
    env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])
}

/// Any type, narrowed to photos and videos (EXTRA_MIME_TYPES).
fn media_types(env: &mut JNIEnv, i: &JObject) -> Result<()> {
    let any = s(env, "*/*")?;
    env.call_method(i, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&any)])?;
    let types = env.new_object_array(2, "java/lang/String", JObject::null())?;
    for (n, t) in ["image/*", "video/*"].into_iter().enumerate() {
        let t = s(env, t)?;
        env.set_object_array_element(&types, n as i32, t)?;
    }
    let key = s(env, "android.intent.extra.MIME_TYPES")?;
    let sig = "(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;";
    env.call_method(i, "putExtra", sig, &[JValue::Object(&key), JValue::Object(&types)])?;
    Ok(())
}

/// The picker closed (`data` is null when nothing was picked). Reads the file off the main thread. After Android closed
/// the app while the picker was open, the chat it was for comes from the store, and opens again.
pub fn picked(env: &mut JNIEnv, act: &JObject, data: &JObject) -> Result<()> {
    inbox::path(env, act)?;
    let gallery = IN_GALLERY.swap(false, Relaxed);
    let mut chat = FOR.lock().unwrap().clone();
    if chat.is_empty() {
        chat = inbox::update(|s| s.picking.clone()).unwrap_or_default();
        if !chat.is_empty() {
            *inbox::OPEN_CHAT.lock().unwrap() = Some(chat.clone());
        }
    }
    let uri = if data.is_null() { JObject::null() } else { env.call_method(data, "getData", "()Landroid/net/Uri;", &[])?.l()? };
    if uri.is_null() && !data.is_null() && gallery && !chat.is_empty() {
        return instead(env, act, &chat); // the Gallery answered with nothing to read
    }
    let Some(dir) = inbox::media_dir().filter(|_| !uri.is_null() && !chat.is_empty()) else {
        inbox::update(|s| s.picked(&chat)); // closed without a pick
        return Ok(());
    };
    set_loading(&chat);
    let (app, act) = (jx::app(env, act)?, env.new_global_ref(act)?);
    let (vm, uri, ctx) = (env.get_java_vm()?, env.new_global_ref(uri)?, env.new_global_ref(app)?);
    std::thread::spawn(move || {
        let Ok(mut env) = vm.attach_current_thread() else { return set_result(&chat, Err("Couldn't read that file".into())) };
        let got = jx::ok(&mut env, |env| read(env, ctx.as_obj(), uri.as_obj())).flatten();
        let kind = got.as_ref().and_then(|(bytes, mime)| Media::kind(mime, bytes));
        // no grant, nothing readable or no type it can tell: the system picker instead, straight away
        if gallery && kind.is_none() && jx::ok(&mut env, |env| instead(env, act.as_obj(), &chat)).is_some() {
            return;
        }
        let pick = match (got, kind) {
            (None, _) => Err("Couldn't read that file".into()),
            (Some((_, mime)), None) => Err(format!("Only photos and videos can be sent ({})", if mime.is_empty() { "unknown type" } else { mime.as_str() })),
            (Some((bytes, _)), Some(k)) if Media::heif(k) => match jx::ok(&mut env, |env| jpeg(env, &bytes)).flatten() {
                Some(jpg) => keep(&dir, &jpg, "image/jpeg"),
                None => Err("Couldn't read that photo".into()),
            },
            (Some((bytes, _)), Some(k)) => keep(&dir, &bytes, k),
        };
        set_result(&chat, pick);
    });
    Ok(())
}

/// A picked file checked (Media::picked) and saved in filesDir/media.
fn keep(dir: &std::path::Path, bytes: &[u8], mime: &str) -> std::result::Result<Media, String> {
    let m = Media::picked(bytes, mime)?;
    let file = dir.join(m.file_name());
    let tmp = file.with_extension("part");
    std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&tmp, bytes)).and_then(|_| std::fs::rename(&tmp, &file)).map_err(|_| "Couldn't save that file".to_string())?;
    Ok(Media { path: file.to_string_lossy().into_owned(), ..m })
}

/// A HEIC / HEIF photo as a JPEG (the PC and the agent take JPEGs), at most 4096 px on its longer side. None: Android
/// couldn't decode it.
fn jpeg(env: &mut JNIEnv, bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let arr = env.byte_array_from_slice(bytes)?;
    let o = env.new_object("android/graphics/BitmapFactory$Options", "()V", &[])?;
    let sig = "([BIILandroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;";
    let len = bytes.len() as i32;
    env.set_field(&o, "inJustDecodeBounds", "Z", JValue::Bool(1))?;
    env.call_static_method("android/graphics/BitmapFactory", "decodeByteArray", sig, &[JValue::Object(&arr), JValue::Int(0), JValue::Int(len), JValue::Object(&o)])?;
    let big = env.get_field(&o, "outWidth", "I")?.i()?.max(env.get_field(&o, "outHeight", "I")?.i()?);
    let mut sample = 1;
    while big / sample > 4096 {
        sample *= 2;
    }
    env.set_field(&o, "inJustDecodeBounds", "Z", JValue::Bool(0))?;
    env.set_field(&o, "inSampleSize", "I", JValue::Int(sample))?;
    let bmp = env.call_static_method("android/graphics/BitmapFactory", "decodeByteArray", sig, &[JValue::Object(&arr), JValue::Int(0), JValue::Int(len), JValue::Object(&o)])?.l()?;
    if bmp.is_null() {
        return Ok(None);
    }
    let out = env.new_object("java/io/ByteArrayOutputStream", "()V", &[])?;
    let fmt = env.get_static_field("android/graphics/Bitmap$CompressFormat", "JPEG", "Landroid/graphics/Bitmap$CompressFormat;")?.l()?;
    let sig = "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z";
    let ok = env.call_method(&bmp, "compress", sig, &[JValue::Object(&fmt), JValue::Int(90), JValue::Object(&out)])?.z()?;
    env.call_method(&bmp, "recycle", "()V", &[])?;
    if !ok {
        return Ok(None);
    }
    let b = env.call_method(&out, "toByteArray", "()[B", &[])?.l()?;
    Ok(Some(env.convert_byte_array(JByteArray::from(b))?))
}

/// The picked file's bytes (read up to just past MEDIA_MAX, enough to know it is too big) and the type it says it is;
/// None when it can't be opened.
fn read(env: &mut JNIEnv, ctx: &JObject, uri: &JObject) -> Result<Option<(Vec<u8>, String)>> {
    let cr = env.call_method(ctx, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
    let t = env.call_method(&cr, "getType", "(Landroid/net/Uri;)Ljava/lang/String;", &[JValue::Object(uri)])?.l()?;
    let mime = jx::string(env, t)?.unwrap_or_default();
    let input = env.call_method(&cr, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(uri)])?.l()?;
    if input.is_null() {
        return Ok(None);
    }
    let buf = env.new_byte_array(1 << 16)?;
    let (mut bytes, mut chunk) = (Vec::new(), vec![0i8; 1 << 16]);
    while (bytes.len() as u64) <= MEDIA_MAX {
        let n = env.call_method(&input, "read", "([B)I", &[JValue::Object(&buf)])?.i()?;
        if n < 0 {
            break;
        }
        env.get_byte_array_region(&buf, 0, &mut chunk[..n as usize])?;
        bytes.extend(chunk[..n as usize].iter().map(|&b| b as u8));
    }
    env.call_method(&input, "close", "()V", &[])?;
    Ok(Some((bytes, mime)))
}

/// A pick under way for `chat` (also PREVIEW, fake_chat.rs); a pick in another chat's box stays as it is.
pub fn set_loading(chat: &str) {
    *PICK.lock().unwrap() = Some((chat.to_string(), Pick::Loading));
    inbox::changed();
}

/// The pick's result: ready, it is kept with its chat; removed while it was loading, it stays removed.
pub fn set_result(chat: &str, r: std::result::Result<Media, String>) {
    let mut p = PICK.lock().unwrap();
    if !matches!(&*p, Some((c, Pick::Loading)) if c == chat) {
        inbox::update(|s| s.picked(chat));
        return;
    }
    *p = match r {
        Ok(m) if inbox::update(|s| s.set_pick(chat, Some(m.clone()))) == Some(true) => None,
        Ok(_) => Some((chat.to_string(), Pick::Failed("Couldn't save that file".into()))),
        Err(why) => {
            inbox::update(|s| s.picked(chat));
            Some((chat.to_string(), Pick::Failed(why)))
        }
    };
    drop(p);
    inbox::changed();
}

/// Removes what was picked in `chat` (the × on the chip), and its file once nothing else uses it.
pub fn clear(chat: &str) {
    let mut p = PICK.lock().unwrap();
    if p.as_ref().is_some_and(|(c, _)| c == chat) {
        *p = None;
    }
    drop(p);
    let unused = inbox::update(|s| {
        let path = s.get(chat).and_then(|c| c.pick.as_ref()).map(|m| m.path.clone());
        s.set_pick(chat, None);
        path.filter(|p| !p.is_empty() && !s.uses(p))
    });
    if let Some(Some(path)) = unused {
        let _ = std::fs::remove_file(path);
    }
    inbox::changed();
}

/// The chip above the field, for the open chat (`pick`: what its store entry holds).
pub fn show(ui: &HomeScreen, chat: &str, pick: Option<&Media>) {
    let c = ui.global::<Chats>();
    let busy = PICK.lock().unwrap().clone().filter(|(c, _)| c == chat).map(|(_, p)| p);
    let (kind, text, pic) = match (&busy, pick) {
        (Some(Pick::Loading), _) => ("loading", "Getting it ready…".into(), None),
        (Some(Pick::Failed(why)), _) => ("error", why.clone(), None),
        (None, None) => ("", String::new(), None),
        (None, Some(m)) => {
            let what = if m.video() { "Video" } else { "Photo" };
            let size = match m.size {
                n if n < 1_048_576 => format!("{} KB", n.div_ceil(1024)),
                n => format!("{:.1} MB", n as f64 / 1_048_576.0),
            };
            let pic = thumb(m).map(|t| t.0);
            (if m.video() { "video" } else { "image" }, format!("{what} · {size}"), pic)
        }
    };
    c.set_pick(kind.into());
    c.set_pick_text(text.into());
    c.set_pick_pic(pic.unwrap_or_default());
}

thread_local! {
    /// Thumbnails already drawn, by file (None: it couldn't be read).
    static THUMBS: RefCell<HashMap<String, Option<(slint::Image, f32)>>> = RefCell::new(HashMap::new());
}

/// A photo, or a video's first frame, at most about `max` pixels on its longer side, as (picture, height / width).
pub fn picture(path: &str, video: bool, max: i32) -> Option<(slint::Image, f32)> {
    let (w, h, px) = android::with(|env, _| env.with_local_frame(16, |env| bitmap(env, path, video, max))).flatten()?;
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&px, w, h);
    Some((slint::Image::from_rgba8_premultiplied(buf), h as f32 / w as f32))
}

/// Decodes with Android (any format it knows; a video through MediaMetadataRetriever) into RGBA bytes.
fn bitmap(env: &mut JNIEnv, path: &str, video: bool, max: i32) -> Result<Option<(u32, u32, Vec<u8>)>> {
    let p = s(env, path)?;
    let bmp = if video {
        let r = env.new_object("android/media/MediaMetadataRetriever", "()V", &[])?;
        env.call_method(&r, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&p)])?;
        let sig = "(JIII)Landroid/graphics/Bitmap;"; // OPTION_CLOSEST_SYNC, scaled to fit max x max
        let frame = env.call_method(&r, "getScaledFrameAtTime", sig, &[JValue::Long(0), JValue::Int(2), JValue::Int(max), JValue::Int(max)])?.l()?;
        env.call_method(&r, "release", "()V", &[])?;
        frame
    } else {
        let o = env.new_object("android/graphics/BitmapFactory$Options", "()V", &[])?;
        let sig = "(Ljava/lang/String;Landroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;";
        env.set_field(&o, "inJustDecodeBounds", "Z", JValue::Bool(1))?;
        env.call_static_method("android/graphics/BitmapFactory", "decodeFile", sig, &[JValue::Object(&p), JValue::Object(&o)])?;
        let big = env.get_field(&o, "outWidth", "I")?.i()?.max(env.get_field(&o, "outHeight", "I")?.i()?);
        let mut sample = 1;
        while big / (sample * 2) >= max {
            sample *= 2;
        }
        env.set_field(&o, "inJustDecodeBounds", "Z", JValue::Bool(0))?;
        env.set_field(&o, "inSampleSize", "I", JValue::Int(sample))?;
        env.call_static_method("android/graphics/BitmapFactory", "decodeFile", sig, &[JValue::Object(&p), JValue::Object(&o)])?.l()?
    };
    if bmp.is_null() {
        return Ok(None);
    }
    // A plain ARGB_8888 copy (video frames can come as RGB_565): its bytes are RGBA, premultiplied.
    let cfg = env.get_static_field("android/graphics/Bitmap$Config", "ARGB_8888", "Landroid/graphics/Bitmap$Config;")?.l()?;
    let soft = env.call_method(&bmp, "copy", "(Landroid/graphics/Bitmap$Config;Z)Landroid/graphics/Bitmap;", &[JValue::Object(&cfg), JValue::Bool(0)])?.l()?;
    env.call_method(&bmp, "recycle", "()V", &[])?;
    if soft.is_null() {
        return Ok(None);
    }
    let (w, h) = (env.call_method(&soft, "getWidth", "()I", &[])?.i()?, env.call_method(&soft, "getHeight", "()I", &[])?.i()?);
    let buf = env.call_static_method("java/nio/ByteBuffer", "allocate", "(I)Ljava/nio/ByteBuffer;", &[JValue::Int(w * h * 4)])?.l()?;
    env.call_method(&soft, "copyPixelsToBuffer", "(Ljava/nio/Buffer;)V", &[JValue::Object(&buf)])?;
    env.call_method(&soft, "recycle", "()V", &[])?;
    let bytes = env.call_method(&buf, "array", "()[B", &[])?.l()?;
    Ok(Some((w as u32, h as u32, env.convert_byte_array(JByteArray::from(bytes))?)))
}

/// Plays a video the PC sent in the phone's video player, through the app's own content link (MediaProvider in
/// Shell.kt) with a one-time read grant.
pub fn play(env: &mut JNIEnv, act: &JObject, path: &str) -> Result<()> {
    let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mime = match name.rsplit('.').next() {
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "video/mp4",
    };
    let i = jx::view_intent(env, "android.intent.action.VIEW", &format!("content://{}.media/{name}", jx::PKG))?;
    let uri = env.call_method(&i, "getData", "()Landroid/net/Uri;", &[])?.l()?;
    let t = s(env, mime)?;
    env.call_method(&i, "setDataAndType", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&uri), JValue::Object(&t)])?;
    env.call_method(&i, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(1)])?; // FLAG_GRANT_READ_URI_PERMISSION
    jx::start_activity(env, act, &i)
}

/// A file's thumbnail, drawn once (a photo, or a video's first frame).
pub fn thumb(m: &Media) -> Option<(slint::Image, f32)> {
    THUMBS.with(|t| t.borrow_mut().entry(m.path.clone()).or_insert_with(|| picture(&m.path, m.video(), 720)).clone())
}
