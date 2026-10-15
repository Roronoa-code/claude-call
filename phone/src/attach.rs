//! A photo or video to send in a chat: the system picker, reading what was picked into filesDir/media (where the
//! PC's files go too, so it shows and plays the same way), and the chip above the field until it is sent or removed.
use crate::{
    Chats, HomeScreen,
    chats::{MEDIA_MAX, Media},
    inbox,
    jx::{self, s},
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JValue},
};
use slint::ComponentHandle;
use std::sync::Mutex;

/// What goes with the next message.
#[derive(Clone, Default)]
enum Pick {
    #[default]
    None,
    Loading,
    Ready(Media),
    Failed(String),
}
static PICK: Mutex<Pick> = Mutex::new(Pick::None);
const REQUEST: i32 = 7; // MainActivity.onActivityResult in Shell.kt

/// Opens the system photo picker (photos and videos); Android 10-12 get "choose a file" limited to them.
pub fn open(env: &mut JNIEnv, act: &JObject) -> Result<()> {
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
    let Some(dir) = inbox::media_dir().filter(|_| !uri.is_null()) else { return Ok(()) };
    set(Pick::Loading);
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
        // Removed while it was loading: stays removed.
        let mut p = PICK.lock().unwrap();
        if matches!(*p, Pick::Loading) {
            *p = pick.map_or_else(Pick::Failed, Pick::Ready);
        }
        drop(p);
        inbox::changed();
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

fn set(p: Pick) {
    *PICK.lock().unwrap() = p;
    inbox::changed();
}

/// PREVIEW (fake_chat.rs): a pick under way, then its result, as `picked` does.
pub fn set_loading() {
    set(Pick::Loading);
}

pub fn set_result(r: std::result::Result<Media, String>) {
    let mut p = PICK.lock().unwrap();
    if matches!(*p, Pick::Loading) {
        *p = r.map_or_else(Pick::Failed, Pick::Ready);
    }
    drop(p);
    inbox::changed();
}

/// Removes what was picked (the × on the chip).
pub fn clear() {
    set(Pick::None);
}

/// Takes what was picked to send with a message: only a file that is ready (the field keeps send off while loading).
pub fn take() -> Option<Media> {
    let mut p = PICK.lock().unwrap();
    let Pick::Ready(m) = std::mem::take(&mut *p) else { return None };
    drop(p);
    inbox::changed();
    Some(m)
}

/// The chip above the field.
pub fn show(ui: &HomeScreen) {
    let c = ui.global::<Chats>();
    let pick = PICK.lock().unwrap().clone();
    let (kind, text, pic) = match &pick {
        Pick::None => ("", String::new(), None),
        Pick::Loading => ("loading", "Getting it ready…".into(), None),
        Pick::Failed(why) => ("error", why.clone(), None),
        Pick::Ready(m) => {
            let what = if m.video() { "Video" } else { "Photo" };
            let size = match m.size {
                n if n < 1_048_576 => format!("{} KB", n.div_ceil(1024)),
                n => format!("{:.1} MB", n as f64 / 1_048_576.0),
            };
            let pic = inbox::thumb(m).map(|t| t.0);
            (if m.video() { "video" } else { "image" }, format!("{what} · {size}"), pic)
        }
    };
    c.set_pick(kind.into());
    c.set_pick_text(text.into());
    c.set_pick_pic(pic.unwrap_or_default());
}
