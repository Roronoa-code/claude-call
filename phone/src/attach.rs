//! A photo or video to send in a chat: the system picker, reading what was picked into filesDir/media (where the
//! PC's files go too, so it shows and plays the same way), and the chip above the field until it is sent or removed.
//! A pick belongs to the chat it was picked in: once ready it is kept with that chat in the store (chats.json), so it
//! never goes with a message in another chat and stays when the app is closed.
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
use std::{cell::RefCell, collections::HashMap, sync::Mutex};

/// A pick on its way, or one that can't be sent (why), and the chat it was picked in. Ready ones are in the store.
#[derive(Clone)]
enum Pick {
    Loading,
    Failed(String),
}
static PICK: Mutex<Option<(String, Pick)>> = Mutex::new(None);
/// The chat the system picker was opened for (its answer comes later, in picked()).
static FOR: Mutex<String> = Mutex::new(String::new());
const REQUEST: i32 = 7; // MainActivity.onActivityResult in Shell.kt

/// Opens the system photo picker (photos and videos) for `chat`; Android 10-12 get "choose a file" limited to them.
pub fn open(env: &mut JNIEnv, act: &JObject, chat: &str) -> Result<()> {
    *FOR.lock().unwrap() = chat.to_string();
    let pick_images = jx::sdk(env)? >= 33;
    let action = s(env, if pick_images { "android.provider.action.PICK_IMAGES" } else { "android.intent.action.GET_CONTENT" })?;
    let i = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
    if !pick_images {
        let (any, openable) = (s(env, "*/*")?, s(env, "android.intent.category.OPENABLE")?);
        env.call_method(&i, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&any)])?;
        env.call_method(&i, "addCategory", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&openable)])?;
        let types = env.new_object_array(2, "java/lang/String", JObject::null())?;
        for (n, t) in ["image/*", "video/*"].into_iter().enumerate() {
            let t = s(env, t)?;
            env.set_object_array_element(&types, n as i32, t)?;
        }
        let key = s(env, "android.intent.extra.MIME_TYPES")?;
        let sig = "(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;";
        env.call_method(&i, "putExtra", sig, &[JValue::Object(&key), JValue::Object(&types)])?;
    }
    env.call_method(act, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&i), JValue::Int(REQUEST)])?;
    Ok(())
}

/// The picker closed (`data` is null when nothing was picked). Reads the file off the main thread.
pub fn picked(env: &mut JNIEnv, act: &JObject, data: &JObject) -> Result<()> {
    if data.is_null() {
        return Ok(());
    }
    let uri = env.call_method(data, "getData", "()Landroid/net/Uri;", &[])?.l()?;
    let chat = FOR.lock().unwrap().clone();
    let Some(dir) = inbox::media_dir().filter(|_| !uri.is_null() && !chat.is_empty()) else { return Ok(()) };
    set_loading(&chat);
    let app = jx::app(env, act)?;
    let (vm, uri, ctx) = (env.get_java_vm()?, env.new_global_ref(uri)?, env.new_global_ref(app)?);
    std::thread::spawn(move || {
        let got = vm.attach_current_thread().ok().and_then(|mut env| jx::ok(&mut env, |env| read(env, ctx.as_obj(), uri.as_obj())));
        let pick = match got.unwrap_or_else(|| Err("Couldn't read that file".into())) {
            Ok((bytes, mime)) => Media::picked(&bytes, &mime).and_then(|m| {
                let file = dir.join(m.file_name());
                let tmp = file.with_extension("part");
                std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&tmp, &bytes)).and_then(|_| std::fs::rename(&tmp, &file)).map_err(|_| "Couldn't save that file".to_string())?;
                Ok(Media { path: file.to_string_lossy().into_owned(), ..m })
            }),
            Err(e) => Err(e),
        };
        set_result(&chat, pick);
    });
    Ok(())
}

/// The picked file's bytes (read up to just past MEDIA_MAX, enough to know it is too big) and its type.
fn read(env: &mut JNIEnv, ctx: &JObject, uri: &JObject) -> Result<std::result::Result<(Vec<u8>, String), String>> {
    let cr = env.call_method(ctx, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
    let t = env.call_method(&cr, "getType", "(Landroid/net/Uri;)Ljava/lang/String;", &[JValue::Object(uri)])?.l()?;
    let mime = jx::string(env, t)?.unwrap_or_default();
    let input = env.call_method(&cr, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(uri)])?.l()?;
    if input.is_null() {
        return Ok(Err("Couldn't read that file".into()));
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
    Ok(Ok((bytes, mime)))
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
        return;
    }
    *p = match r {
        Ok(m) if inbox::update(|s| s.set_pick(chat, Some(m.clone()))) == Some(true) => None,
        Ok(_) => Some((chat.to_string(), Pick::Failed("Couldn't save that file".into()))),
        Err(why) => Some((chat.to_string(), Pick::Failed(why))),
    };
    drop(p);
    inbox::changed();
}

/// Removes what was picked in `chat` (the × on the chip).
pub fn clear(chat: &str) {
    let mut p = PICK.lock().unwrap();
    if p.as_ref().is_some_and(|(c, _)| c == chat) {
        *p = None;
    }
    drop(p);
    inbox::update(|s| s.set_pick(chat, None));
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
