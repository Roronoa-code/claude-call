package fyi.mani.callme

import android.app.Activity
import android.app.NativeActivity
import android.app.Service
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.window.OnBackInvokedDispatcher

/**
 * The whole app is the Rust library (phone/ in the repo, libcallme_phone.so). Android must create these classes
 * itself, so each one only forwards to it (phone/src/shell.rs).
 */
object Native {
    init { System.loadLibrary("callme_phone") }
    @JvmStatic external fun foreground(service: Service)
    @JvmStatic external fun stop()
    @JvmStatic external fun start(context: Context)
    @JvmStatic external fun stopRinging(context: Context, handled: Boolean)
    @JvmStatic external fun callOpened(activity: Activity, intent: Intent)
    @JvmStatic external fun callBack(activity: Activity)
    @JvmStatic external fun callClosed(activity: Activity)
    @JvmStatic external fun homeOpened(activity: Activity)
    @JvmStatic external fun homeResumed(activity: Activity)
    @JvmStatic external fun permissionsAnswered(activity: Activity)
    @JvmStatic external fun declined(context: Context, intent: Intent)
    @JvmStatic external fun modeTapped(context: Context, intent: Intent)
    @JvmStatic external fun widgets(context: Context)
}

/** The home screen (phone/src/home.rs). */
class MainActivity : NativeActivity() {
    override fun onCreate(savedInstanceState: Bundle?) { super.onCreate(savedInstanceState); Native.homeOpened(this) }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); setIntent(intent); Native.homeOpened(this) }
    override fun onResume() { super.onResume(); if (!isFinishing) Native.homeResumed(this) }
    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) = Native.permissionsAnswered(this)
}

/** The call screen (phone/src/screen.rs). */
class CallActivity : NativeActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Native.callOpened(this, intent)
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { Native.callBack(this) }
        }
    }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); Native.callOpened(this, intent) }
    override fun onUserInteraction() { super.onUserInteraction(); Native.stopRinging(this, true) }
    @Deprecated("Used on Android 12 and earlier")
    override fun onBackPressed() = Native.callBack(this)
    override fun onDestroy() { Native.callClosed(this); super.onDestroy() }
}

/** Keeps the call listener running (phone/src/service.rs). */
class ListenService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int { Native.foreground(this); return START_STICKY }
    override fun onDestroy() { Native.stop(); super.onDestroy() }
}

class DeclineReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.declined(context, intent)
}

/** After a reboot or an app update, which otherwise leave the listener stopped until the app is opened. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.start(context)
}

/** Widget tile taps. Not exported, so only this app's own taps can change the mode. */
class ModeTap : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Native.modeTapped(context, intent)
}

/** The contact-mode widget (phone/src/widget.rs). */
open class ModeWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) = Native.widgets(context)
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) = Native.widgets(context)
}

/** Same widget, offered at 4x2 in the picker. */
class ModeWidgetWide : ModeWidget()
