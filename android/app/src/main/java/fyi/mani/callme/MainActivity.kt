package fyi.mani.callme

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.NotificationManager
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import android.util.TypedValue
import android.view.Gravity
import android.view.WindowInsets
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/** One-time setup: permissions, the ntfy topic, and starting the always-on listener. */
class MainActivity : Activity() {
    private lateinit var status: TextView
    private lateinit var topic: EditText
    private var modeRowRef: List<Pair<Button, String>> = emptyList()

    /** Marks the selected contact mode. */
    private fun showMode(buttons: List<Pair<Button, String>>) {
        val current = Prefs.mode(this)
        buttons.forEach { (b, mode) -> b.alpha = if (mode == current) 1f else 0.45f; b.isSelected = mode == current }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pad = dp(20)
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#111214"))
            setOnApplyWindowInsetsListener { v, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
                v.setPadding(pad + bars.left, pad + bars.top, pad + bars.right, pad + bars.bottom)
                insets
            }
        }
        root.addView(text("Claude Call", 28f, "#F2F2F3"))
        root.addView(text("Claude rings this phone when it needs you.", 16f, "#9A9CA3"))
        status = text("", 16f, "#F2F2F3").apply { setPadding(0, dp(20), 0, dp(20)) }
        root.addView(status)
        root.addView(text("How agents can reach me", 14f, "#9A9CA3"))
        val modes = listOf("call" to "Calls", "text" to "Texts", "both" to "Both", "none" to "None")
        val modeRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val modeButtons = modes.map { (mode, label) ->
            Button(this).apply {
                text = label
                layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                setOnClickListener {
                    Prefs.setMode(this@MainActivity, mode); ContactMode.publish(this@MainActivity, mode)
                    showMode(modeRowRef); ModeWidget.updateAll(this@MainActivity)
                }
            }.also { modeRow.addView(it) }
        }
        modeRowRef = modeButtons.zip(modes.map { it.first })
        showMode(modeRowRef)
        root.addView(modeRow)
        root.addView(text("Channel", 14f, "#9A9CA3").apply { setPadding(0, dp(16), 0, 0) })
        topic = EditText(this).apply {
            setText(Prefs.topic(this@MainActivity))
            setTextColor(Color.parseColor("#F2F2F3"))
            setSingleLine()
        }
        root.addView(topic)
        root.addView(button("Set up permissions") { nextPermission() })
        root.addView(button("Save and start") {
            Prefs.setTopic(this, topic.text.toString().trim())
            stopService(Intent(this, ListenService::class.java))
            ListenService.start(this)
            refresh()
        })
        if (getSystemService(AppWidgetManager::class.java).isRequestPinAppWidgetSupported) {
            val widgets = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            listOf("Add small widget" to ModeWidget::class.java, "Add wide widget" to ModeWidgetWide::class.java).forEachIndexed { i, (label, cls) ->
                widgets.addView(button(label) {
                    getSystemService(AppWidgetManager::class.java).requestPinAppWidget(ComponentName(this, cls), null, null)
                }.apply {
                    layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
                        .apply { topMargin = dp(12); if (i == 0) marginEnd = dp(6) else marginStart = dp(6) }
                })
            }
            root.addView(widgets)
        }
        root.addView(button("Test ring") {
            Ringer.ring(applicationContext, CallActivity.TEST_LINK, "Test ring")
        })
        setContentView(root)
        returnToCall()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        returnToCall()
    }

    private fun returnToCall() {
        val link = ListenService.activeCallLink ?: return
        startActivity(CallActivity.intent(this, link, answer = true))
        finish()
    }

    override fun onResume() {
        super.onResume()
        if (isFinishing) return
        refresh()
        if (missing().isEmpty()) ListenService.start(this)
        // Show a mode changed elsewhere (Claude or Discord).
        Thread {
            ContactMode.latest(this)?.let { (m, at) -> ContactMode.received(this, m, at); runOnUiThread { showMode(modeRowRef) } }
        }.start()
    }

    private fun missing(): List<String> {
        val nm = getSystemService(NotificationManager::class.java)
        val pm = getSystemService(PowerManager::class.java)
        return buildList {
            if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) add("Notifications")
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) add("Microphone")
            if (Build.VERSION.SDK_INT >= 34 && !nm.canUseFullScreenIntent()) add("Full-screen calls")
            if (!pm.isIgnoringBatteryOptimizations(packageName)) add("Battery: unrestricted")
        }
    }

    private fun refresh() {
        val m = missing()
        status.text = if (m.isEmpty()) "✓ All set. Claude can call you." else "Still needed: ${m.joinToString(", ")}"
    }

    /** Walks through whatever is still missing, one system screen at a time. */
    @SuppressLint("BatteryLife")
    private fun nextPermission() {
        val m = missing()
        when {
            "Notifications" in m || "Microphone" in m ->
                requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS, Manifest.permission.RECORD_AUDIO), 1)
            "Full-screen calls" in m ->
                startActivity(Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, Uri.parse("package:$packageName")))
            "Battery: unrestricted" in m ->
                startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$packageName")))
            else -> refresh()
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        refresh()
        if (missing().isNotEmpty()) nextPermission()
    }

    private fun dp(v: Int) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics).toInt()
    private fun text(s: String, size: Float, color: String) = TextView(this).apply {
        text = s; textSize = size; setTextColor(Color.parseColor(color))
    }
    private fun button(label: String, onClick: () -> Unit) = Button(this).apply {
        text = label
        layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = dp(12) }
        gravity = Gravity.CENTER
        setOnClickListener { onClick() }
    }
}
