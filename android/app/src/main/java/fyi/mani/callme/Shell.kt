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
import android.text.Editable
import android.view.Surface
import android.view.inputmethod.InputMethodManager
import android.view.SurfaceHolder
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
    @JvmStatic external fun homeShown(shown: Boolean)
    @JvmStatic external fun permissionsAnswered(activity: Activity)
    @JvmStatic external fun picked(activity: Activity, data: Intent?)
    @JvmStatic external fun declined(context: Context, intent: Intent)
    @JvmStatic external fun modeTapped(context: Context, intent: Intent)
    @JvmStatic external fun widgets(context: Context)
}

/**
 * Keeps Android's starting window (the app's dark background, themes.xml) on screen until Rust has drawn a frame
 * into the new surface (live_gl.rs). Without it the window counted as drawn while still empty, so a tap on the icon
 * showed the launcher until the GPU's first frame popped the app in.
 */
open class Shown : NativeActivity() {
    @JvmField @Volatile var waiting = false
    private var finish: Runnable? = null
    override fun surfaceRedrawNeededAsync(holder: SurfaceHolder, drawingFinished: Runnable) {
        finish?.run()
        finish = drawingFinished
        waiting = true
        surfaceRedrawNeeded(holder) // Rust draws now
        window.decorView.postDelayed({ drawn() }, 3000) // never held by a frame that doesn't come
    }
    fun drawn() = runOnUiThread { waiting = false; finish?.run(); finish = null }

    // How often the screen's picture changes, told to the display so it can slow down while it rests (home.rs);
    // 0: no preference. A touch lifts it at once.
    private var surface: Surface? = null
    private var fps = 0f
    override fun surfaceCreated(holder: SurfaceHolder) { super.surfaceCreated(holder); surface = holder.surface; fps = 0f }
    override fun surfaceDestroyed(holder: SurfaceHolder) { surface = null; super.surfaceDestroyed(holder) }
    fun rate(want: Float) = runOnUiThread {
        if (want != fps && Build.VERSION.SDK_INT >= 30) {
            fps = want
            surface?.takeIf { it.isValid }?.setFrameRate(want, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
        }
    }
    override fun onUserInteraction() { super.onUserInteraction(); rate(0f) }

    /**
     * Empties the text field's copy of the text on the keyboard's side (Slint's input view) and restarts the keyboard
     * on it, as a text box does when the app sets its text: a message was just sent while the keyboard was still
     * putting a word together, and that word must not come back in the empty field.
     */
    fun clearField() = runOnUiThread {
        val v = currentFocus ?: return@runOnUiThread
        val f = runCatching { v.javaClass.getDeclaredField("mEditable").apply { isAccessible = true } }.getOrNull() ?: return@runOnUiThread
        (f.get(v) as? Editable)?.clear()
        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).restartInput(v)
    }
}

/** The home screen (phone/src/home.rs). */
class MainActivity : Shown() {
    override fun onCreate(savedInstanceState: Bundle?) { super.onCreate(savedInstanceState); Native.homeOpened(this) }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); setIntent(intent); Native.homeOpened(this) }
    override fun onResume() { super.onResume(); if (!isFinishing) Native.homeResumed(this) }
    override fun onStart() { super.onStart(); Native.homeShown(true) }
    override fun onStop() { Native.homeShown(false); super.onStop() }
    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, results: IntArray) = Native.permissionsAnswered(this)
    @Deprecated("One picker: the result API needs androidx")
    override fun onActivityResult(code: Int, result: Int, data: Intent?) { super.onActivityResult(code, result, data); if (code == 7) Native.picked(this, data) }
}

/** The call screen (phone/src/screen.rs). */
class CallActivity : Shown() {
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
