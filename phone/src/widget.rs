//! The "Aura" home-screen widget: four tiles for the contact mode, the chosen one lit in its colour; the wide one
//! also says what the mode means and when it changed. RemoteViews over the XML layouts in res/layout.
use crate::{
    jx::{self, PKG, call, s},
    settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JValue},
};

/// id, tile label, wide card's title and line (copy and colours from the approved widget preview).
const MODES: [(&str, &str, &str, &str); 4] = [
    ("call", "Calls", "Calls only", "Claude can call you"),
    ("text", "Texts", "Texts only", "Claude can message you"),
    ("both", "Both", "Calls and texts", "Claude can call or message you"),
    ("none", "None", "Do not disturb", "Claude saves its questions"),
];
const WHITE: i32 = 0xFFFFFFFFu32 as i32;
const DIM: i32 = 0x8CFFFFFFu32 as i32; // white at 55%
const DIP: i32 = 1; // TypedValue.COMPLEX_UNIT_DIP
const SP: i32 = 2;
// The preview's cards (412dp screen): 2x2 is 178x212, 4x2 is 380x212. One UI cells are taller, so the card keeps the
// preview's shape (a touch wider keeps the tiles near square), is centred in the cell, and its contents scale with it.
const PREVIEW_H: f32 = 212.0;
const COMPACT_ASPECT: f32 = 0.92;
const WIDE_ASPECT: f32 = 2.0;
const WIDE_MIN_WIDTH: f32 = 300.0; // below this the title no longer fits beside the tiles

pub fn component<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, class: &str) -> Result<JObject<'a>> {
    let c = s(env, &format!("{PKG}.{class}"))?;
    env.new_object("android/content/ComponentName", "(Landroid/content/Context;Ljava/lang/String;)V", &[JValue::Object(ctx), JValue::Object(&c)])
}

/// Redraws every placed widget, both sizes, from the settings.
pub fn update_all(env: &mut JNIEnv, ctx: &JObject) -> Result<()> {
    let mgr = env.call_static_method("android/appwidget/AppWidgetManager", "getInstance", "(Landroid/content/Context;)Landroid/appwidget/AppWidgetManager;", &[JValue::Object(ctx)])?.l()?;
    for class in ["ModeWidget", "ModeWidgetWide"] {
        let cn = component(env, ctx, class)?;
        let ids = env.call_method(&mgr, "getAppWidgetIds", "(Landroid/content/ComponentName;)[I", &[JValue::Object(&cn)])?.l()?;
        let ids = jni::objects::JIntArray::from(ids);
        let mut buf = vec![0; env.get_array_length(&ids)? as usize];
        env.get_int_array_region(&ids, 0, &mut buf)?;
        for id in buf {
            let opts = env.call_method(&mgr, "getAppWidgetOptions", "(I)Landroid/os/Bundle;", &[JValue::Int(id)])?.l()?;
            let v = views(env, ctx, &opts)?;
            env.call_method(&mgr, "updateAppWidget", "(ILandroid/widget/RemoteViews;)V", &[JValue::Int(id), JValue::Object(&v)])?;
        }
    }
    Ok(())
}

fn size_f<'a>(env: &mut JNIEnv<'a>, w: f32, h: f32) -> Result<JObject<'a>> {
    env.new_object("android/util/SizeF", "(FF)V", &[JValue::Float(w), JValue::Float(h)])
}

/// One layout per size the launcher may show.
fn views<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, opts: &JObject) -> Result<JObject<'a>> {
    if jx::sdk(env)? < 31 {
        return build(env, ctx, false, None);
    }
    let key = s(env, "appWidgetSizes")?; // AppWidgetManager.OPTION_APPWIDGET_SIZES
    let sizes = env.call_method(opts, "getParcelableArrayList", "(Ljava/lang/String;)Ljava/util/ArrayList;", &[JValue::Object(&key)])?.l()?;
    let n = if sizes.is_null() { 0 } else { env.call_method(&sizes, "size", "()I", &[])?.i()? };
    let map = env.new_object("java/util/HashMap", "()V", &[])?;
    let add = |env: &mut JNIEnv, size: &JObject, v: &JObject| -> Result<()> {
        env.call_method(&map, "put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;", &[JValue::Object(size), JValue::Object(v)])?;
        Ok(())
    };
    if n == 0 {
        // Not measured yet: a compact and a wide layout.
        let (a, b) = (size_f(env, 100.0, 100.0)?, size_f(env, WIDE_MIN_WIDTH, 100.0)?);
        let (va, vb) = (build(env, ctx, false, None)?, build(env, ctx, true, None)?);
        add(env, &a, &va)?;
        add(env, &b, &vb)?;
    }
    for i in 0..n {
        let sz = env.call_method(&sizes, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])?.l()?;
        let w = env.call_method(&sz, "getWidth", "()F", &[])?.f()?;
        let h = env.call_method(&sz, "getHeight", "()F", &[])?.f()?;
        let v = build(env, ctx, w >= WIDE_MIN_WIDTH, Some((w, h)))?;
        add(env, &sz, &v)?;
    }
    env.new_object("android/widget/RemoteViews", "(Ljava/util/Map;)V", &[JValue::Object(&map)])
}

fn text(env: &mut JNIEnv, v: &JObject, method: &str, id: i32, t: &str) -> Result<()> {
    let t = s(env, t)?;
    call(env, v, method, "(ILjava/lang/CharSequence;)V", &[JValue::Int(id), JValue::Object(&t)])
}

fn layout_size(env: &mut JNIEnv, v: &JObject, id: i32, w: Option<f32>, h: Option<f32>) -> Result<()> {
    if let Some(w) = w {
        call(env, v, "setViewLayoutWidth", "(IFI)V", &[JValue::Int(id), JValue::Float(w), JValue::Int(DIP)])?;
    }
    if let Some(h) = h {
        call(env, v, "setViewLayoutHeight", "(IFI)V", &[JValue::Int(id), JValue::Float(h), JValue::Int(DIP)])?;
    }
    Ok(())
}

fn build<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, wide: bool, size: Option<(f32, f32)>) -> Result<JObject<'a>> {
    let current = settings::mode(env, ctx)?;
    let new_api = jx::sdk(env)? >= 31;
    let layout = jx::res(env, ctx, "layout", if wide { "widget_mode_wide" } else { "widget_mode_compact" })?;
    let pkg = s(env, PKG)?;
    let v = env.new_object("android/widget/RemoteViews", "(Ljava/lang/String;I)V", &[JValue::Object(&pkg), JValue::Int(layout)])?;
    let mut k = 1.0; // scale against the preview card
    if let (Some((w, h)), true) = (size, new_api) {
        let card_h = h.min(w / if wide { WIDE_ASPECT } else { COMPACT_ASPECT });
        k = (card_h / PREVIEW_H).clamp(0.8, 1.4);
        let bg = env.get_static_field("android/R$id", "background", "I")?.i()?;
        layout_size(env, &v, bg, None, Some(card_h))?;
    }
    let density = jx::density(env, ctx)?;
    let px = |dp: f32| JValue::Int((dp * k * density) as i32);
    let tile_bg = jx::res(env, ctx, "drawable", "widget_tile")?;
    for (n, (id, label, ..)) in MODES.iter().enumerate() {
        let on = *id == current;
        let res = |env: &mut JNIEnv, kind: &str, name: &str| jx::res(env, ctx, kind, &format!("{name}_{id}"));
        let (tile, icon, lbl) = (res(env, "id", "tile")?, res(env, "id", "icon")?, res(env, "id", "label")?);
        let bg = if !on { tile_bg } else { res(env, "drawable", if wide { "widget_outline" } else { "widget_lit" })? };
        let set_bg = s(env, "setBackgroundResource")?;
        call(env, &v, "setInt", "(ILjava/lang/String;I)V", &[JValue::Int(tile), JValue::Object(&set_bg), JValue::Int(bg)])?;
        // A white glyph at 55%, like the label (a tint was ignored on One UI).
        let alpha = s(env, "setImageAlpha")?;
        call(env, &v, "setInt", "(ILjava/lang/String;I)V", &[JValue::Int(icon), JValue::Object(&alpha), JValue::Int(if on { 255 } else { 140 })])?;
        call(env, &v, "setTextColor", "(II)V", &[JValue::Int(lbl), JValue::Int(if on { WHITE } else { DIM })])?;
        call(env, &v, "setTextViewTextSize", "(IIF)V", &[JValue::Int(lbl), JValue::Int(SP), JValue::Float(14.0 * k)])?;
        text(env, &v, "setContentDescription", tile, &if on { format!("{label}, selected") } else { label.to_string() })?;
        let tap = tap(env, ctx, id, n as i32)?;
        call(env, &v, "setOnClickPendingIntent", "(ILandroid/app/PendingIntent;)V", &[JValue::Int(tile), JValue::Object(&tap)])?;
        if !wide {
            call(env, &v, "setViewPadding", "(IIIII)V", &[JValue::Int(tile), px(12.0), px(12.0), px(12.0), px(10.0)])?;
        }
        if new_api {
            let d = (if wide { 20.0 } else if on { 26.0 } else { 22.0 }) * k;
            layout_size(env, &v, icon, Some(d), Some(d))?;
        }
    }
    if wide {
        let (id, _, title, line) = MODES.iter().find(|m| m.0 == current).unwrap_or(&MODES[2]);
        let r = |env: &mut JNIEnv, kind: &str, name: &str| jx::res(env, ctx, kind, name);
        let (hero, hero_icon, hero_title) = (r(env, "id", "hero")?, r(env, "id", "hero_icon")?, r(env, "id", "hero_title")?);
        let (hero_line, hero_since) = (r(env, "id", "hero_line")?, r(env, "id", "hero_since")?);
        let (lit, glyph) = (r(env, "drawable", &format!("widget_lit_{id}"))?, r(env, "drawable", &format!("glyph_{id}"))?);
        let set_bg = s(env, "setBackgroundResource")?;
        call(env, &v, "setInt", "(ILjava/lang/String;I)V", &[JValue::Int(hero), JValue::Object(&set_bg), JValue::Int(lit)])?;
        call(env, &v, "setViewPadding", "(IIIII)V", &[JValue::Int(hero), px(14.0), px(14.0), px(14.0), px(12.0)])?;
        call(env, &v, "setImageViewResource", "(II)V", &[JValue::Int(hero_icon), JValue::Int(glyph)])?;
        text(env, &v, "setTextViewText", hero_title, title)?;
        text(env, &v, "setTextViewText", hero_line, line)?;
        call(env, &v, "setTextViewTextSize", "(IIF)V", &[JValue::Int(hero_line), JValue::Int(SP), JValue::Float(13.0 * k)])?;
        call(env, &v, "setTextViewTextSize", "(IIF)V", &[JValue::Int(hero_since), JValue::Int(SP), JValue::Float(12.0 * k)])?;
        if new_api {
            layout_size(env, &v, hero_icon, Some(30.0 * k), Some(30.0 * k))?;
            layout_size(env, &v, hero_title, None, Some(23.0 * k))?; // the title auto-sizes to this
        }
        let at = settings::mode_changed_at(env, ctx)?;
        call(env, &v, "setViewVisibility", "(II)V", &[JValue::Int(hero_since), JValue::Int(if at > 0 { 0 } else { 8 })])?;
        if at > 0 {
            let h24 = env.call_static_method("android/text/format/DateFormat", "is24HourFormat", "(Landroid/content/Context;)Z", &[JValue::Object(ctx)])?.z()?;
            let pattern = s(env, if h24 { "H:mm" } else { "h:mm a" })?;
            let sig = "(Ljava/lang/CharSequence;J)Ljava/lang/CharSequence;";
            let t = env.call_static_method("android/text/format/DateFormat", "format", sig, &[JValue::Object(&pattern), JValue::Long(at)])?.l()?;
            let t = env.call_method(&t, "toString", "()Ljava/lang/String;", &[])?.l()?;
            let t = jx::string(env, t)?.unwrap_or_default();
            text(env, &v, "setTextViewText", hero_since, &format!("Changed {t}"))?;
        }
    }
    Ok(v)
}

/// A tile tap: ModeTap (not exported, so only this app's own taps can change the mode).
fn tap<'a>(env: &mut JNIEnv<'a>, ctx: &JObject, mode: &str, n: i32) -> Result<JObject<'a>> {
    let i = jx::intent(env, ctx, "ModeTap")?;
    let u = s(env, &format!("callme://mode/{mode}"))?;
    let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&u)])?.l()?;
    env.call_method(&i, "setData", "(Landroid/net/Uri;)Landroid/content/Intent;", &[JValue::Object(&uri)])?;
    jx::pending(env, ctx, "getBroadcast", 10 + n, &i)
}

/// ModeTap: the mode named by the tapped tile's link.
pub fn tapped(env: &mut JNIEnv, ctx: &JObject, intent: &JObject) -> Result<()> {
    let uri = env.call_method(intent, "getData", "()Landroid/net/Uri;", &[])?.l()?;
    if uri.is_null() {
        return Ok(());
    }
    let m = env.call_method(&uri, "getLastPathSegment", "()Ljava/lang/String;", &[])?.l()?;
    match jx::string(env, m)? {
        Some(m) => settings::choose(env, ctx, &m),
        None => Ok(()),
    }
}
