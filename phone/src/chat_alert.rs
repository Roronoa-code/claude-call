//! Chat notifications: one per chat (a reply, an agent's question or an update), grouped, and gone once read.
use crate::{
    jx::{self, s, service},
    ring, settings,
};
use jni::{
    JNIEnv,
    errors::Result,
    objects::{JObject, JObjectArray, JValue},
};

fn notification_id(chat: &str) -> i32 {
    100 + (chat.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) & 0xf_ffff) as i32
}

/// Chat notifications are their own group, with a summary that opens the newest chat. Left ungrouped, Android bundled
/// them with "Ready for Claude's calls", and a tap on the bundle opened the home screen instead of the chat.
const GROUP: &str = "chats";
const SUMMARY_ID: i32 = 99; // chat notifications are 100 and up

/// A chat was read: its notification goes, and the summary too once no chat notification is left.
pub fn cancel(env: &mut JNIEnv, ctx: &JObject, chat: &str) -> Result<()> {
    let nm = service(env, ctx, "notification")?;
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(notification_id(chat))])?;
    let all = JObjectArray::from(env.call_method(&nm, "getActiveNotifications", "()[Landroid/service/notification/StatusBarNotification;", &[])?.l()?);
    for i in 0..env.get_array_length(&all)? {
        let n = env.get_object_array_element(&all, i)?;
        if env.call_method(&n, "getId", "()I", &[])?.i()? > SUMMARY_ID {
            return Ok(());
        }
    }
    env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(SUMMARY_ID)])?;
    Ok(())
}

/// Shows what came in a chat (`name`: who, `text`: what); a tap opens that chat.
pub fn notify(env: &mut JNIEnv, ctx: &JObject, chat: &str, name: &str, text: &str) -> Result<()> {
    let nm = service(env, ctx, "notification")?;
    // Quiet modes post on a silent low-importance channel (Notification.Builder has no public setSilent: calling it
    // threw, so no reply was ever shown).
    let quiet = matches!(settings::mode(env, ctx)?.as_str(), "none" | "sleep");
    let channel_id = if quiet { "chat_replies_quiet" } else { "chat_replies" };
    if quiet {
        ring::channel(env, &nm, channel_id, "Chat replies (quiet)", 2, true)?; // IMPORTANCE_LOW
    } else {
        ring::channel(env, &nm, channel_id, "Chat replies", 4, false)?; // IMPORTANCE_HIGH
    }
    let home = jx::intent(env, ctx, "MainActivity")?;
    env.call_method(&home, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(0x0400_0000 | 0x2000_0000 | 0x1000_0000)])?; // CLEAR_TOP | SINGLE_TOP | NEW_TASK
    let c = s(env, chat)?;
    jx::put(env, &home, "chat", JValue::Object(&c))?;
    let id = notification_id(chat);
    let tap = jx::pending(env, ctx, "getActivity", id, &home)?;
    let builder = "Landroid/app/Notification$Builder;";
    let (channel, group) = (s(env, channel_id)?, s(env, GROUP)?);
    let icon = env.get_static_field("android/R$drawable", "sym_action_chat", "I")?.i()?;
    // The message, then the group's summary (silent: the message alerts) with the same tap.
    for summary in [false, true] {
        let b = env.new_object("android/app/Notification$Builder", "(Landroid/content/Context;Ljava/lang/String;)V", &[JValue::Object(ctx), JValue::Object(&channel)])?;
        jx::call(env, &b, "setSmallIcon", &format!("(I){builder}"), &[JValue::Int(icon)])?;
        ring::set_text(env, &b, "setContentTitle", name)?;
        ring::set_text(env, &b, "setContentText", text)?;
        jx::call(env, &b, "setContentIntent", &format!("(Landroid/app/PendingIntent;){builder}"), &[JValue::Object(&tap)])?;
        // A tapped summary stays (cancelling it would clear every chat's reply); reading the last chat removes it.
        jx::call(env, &b, "setAutoCancel", &format!("(Z){builder}"), &[JValue::Bool(u8::from(!summary))])?;
        jx::call(env, &b, "setGroup", &format!("(Ljava/lang/String;){builder}"), &[JValue::Object(&group)])?;
        if summary {
            jx::call(env, &b, "setGroupSummary", &format!("(Z){builder}"), &[JValue::Bool(1)])?;
            jx::call(env, &b, "setGroupAlertBehavior", &format!("(I){builder}"), &[JValue::Int(2)])?; // GROUP_ALERT_CHILDREN
        } else {
            let big = env.new_object("android/app/Notification$BigTextStyle", "()V", &[])?;
            let t = s(env, text)?;
            env.call_method(&big, "bigText", "(Ljava/lang/CharSequence;)Landroid/app/Notification$BigTextStyle;", &[JValue::Object(&t)])?;
            jx::call(env, &b, "setStyle", &format!("(Landroid/app/Notification$Style;){builder}"), &[JValue::Object(&big)])?;
            let cat = s(env, "msg")?;
            jx::call(env, &b, "setCategory", &format!("(Ljava/lang/String;){builder}"), &[JValue::Object(&cat)])?;
        }
        let n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
        let nid = if summary { SUMMARY_ID } else { id };
        env.call_method(&nm, "notify", "(ILandroid/app/Notification;)V", &[JValue::Int(nid), JValue::Object(&n)])?;
    }
    Ok(())
}
