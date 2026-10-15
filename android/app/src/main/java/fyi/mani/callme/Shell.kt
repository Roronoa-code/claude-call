package fyi.mani.callme

import android.app.Activity
import android.app.NativeActivity
import android.app.Service
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File
import java.io.FileNotFoundException
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
    @JvmStatic external fun stopRinging(context: Context)
    @JvmStatic external fun callOpened(activity: Activity, intent: Intent)
    @JvmStatic external fun callBack(activity: Activity)
    @JvmStatic external fun callClosed(activity: Activity)
    @JvmStatic external fun homeOpened(activity: Activity)
    @JvmStatic external fun homeResumed(activity: Activity)
    @JvmStatic external fun permissionsAnswered(activity: Activity)
    @JvmStatic external fun picked(activity: Activity, data: Intent?)
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
    @Deprecated("One picker: the result API needs androidx")
    override fun onActivityResult(code: Int, result: Int, data: Intent?) { super.onActivityResult(code, result, data); if (code == 7) Native.picked(this, data) }
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
    override fun onUserInteraction() { super.onUserInteraction(); Native.stopRinging(this) }
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

/**
 * Lets the video player read a video sent in a chat, by the PC or from this phone (filesDir/media, phone/src/inbox.rs). Not exported: only
 * the one-time read grant on the link the app opens reaches a file, and only a plain name inside that folder.
 */
class MediaProvider : ContentProvider() {
    private fun file(uri: Uri): File? {
        val dir = File(context!!.filesDir, "media")
        val f = File(dir, uri.lastPathSegment ?: return null)
        return f.takeIf { it.parentFile?.canonicalPath == dir.canonicalPath && it.isFile }
    }
    override fun onCreate() = true
    override fun getType(uri: Uri): String? = when (uri.lastPathSegment?.substringAfterLast('.')) {
        "mp4" -> "video/mp4"; "mov" -> "video/quicktime"; "webm" -> "video/webm"
        "png" -> "image/png"; "jpg" -> "image/jpeg"; "webp" -> "image/webp"; "gif" -> "image/gif"
        else -> null
    }
    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw SecurityException("read only")
        return ParcelFileDescriptor.open(file(uri) ?: throw FileNotFoundException(), ParcelFileDescriptor.MODE_READ_ONLY)
    }
    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, order: String?): Cursor? {
        val f = file(uri) ?: return null
        return MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)).apply { addRow(arrayOf<Any>(f.name, f.length())) }
    }
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?) = 0
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
}
