package fyi.mani.callme

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.ColorStateList
import android.net.Uri
import android.os.Build
import android.text.format.DateFormat
import android.util.SizeF
import android.util.TypedValue
import android.view.View
import android.widget.RemoteViews

/** "Aura" home-screen widget: four tiles for the contact mode, the chosen one lit in its colour. */
open class ModeWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        ids.forEach { manager.updateAppWidget(it, views(context)) }
    }

    private class Mode(
        val id: String, val label: String, val title: String, val line: String,
        val glyph: Int, val lit: Int, val outline: Int, val tile: Int, val icon: Int, val text: Int,
    )

    companion object {
        private const val WHITE = 0xFFFFFFFF.toInt()
        private const val DIM = 0x8CFFFFFF.toInt() // white at 55%

        // Copy and colours from the approved preview (design-preview/src/widgets.jsx).
        private val MODES = listOf(
            Mode("call", "Calls", "Calls only", "Claude can call you",
                R.drawable.glyph_call, R.drawable.widget_lit_call, R.drawable.widget_outline_call, R.id.tile_call, R.id.icon_call, R.id.label_call),
            Mode("text", "Texts", "Texts only", "Claude can message you",
                R.drawable.glyph_text, R.drawable.widget_lit_text, R.drawable.widget_outline_text, R.id.tile_text, R.id.icon_text, R.id.label_text),
            Mode("both", "Both", "Calls and texts", "Claude can call or message you",
                R.drawable.glyph_both, R.drawable.widget_lit_both, R.drawable.widget_outline_both, R.id.tile_both, R.id.icon_both, R.id.label_both),
            Mode("none", "None", "Do not disturb", "Claude saves its questions",
                R.drawable.glyph_none, R.drawable.widget_lit_none, R.drawable.widget_outline_none, R.id.tile_none, R.id.icon_none, R.id.label_none),
        )

        /** Redraws every placed widget (both sizes) from Prefs. */
        fun updateAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            listOf(ModeWidget::class.java, ModeWidgetWide::class.java).forEach { cls ->
                manager.getAppWidgetIds(ComponentName(context, cls)).forEach { manager.updateAppWidget(it, views(context)) }
            }
        }

        /** 2x2 tiles, or the 4x2 card once the widget is wide enough (API 31+ picks by size). */
        private fun views(context: Context): RemoteViews {
            val compact = build(context, R.layout.widget_mode_compact)
            if (Build.VERSION.SDK_INT < 31) return compact
            return RemoteViews(mapOf(SizeF(100f, 100f) to compact, SizeF(220f, 100f) to build(context, R.layout.widget_mode_wide)))
        }

        private fun build(context: Context, layout: Int): RemoteViews {
            val wide = layout == R.layout.widget_mode_wide
            val current = Prefs.mode(context)
            val v = RemoteViews(context.packageName, layout)
            MODES.forEach { m ->
                val on = m.id == current
                v.setInt(m.tile, "setBackgroundResource", if (!on) R.drawable.widget_tile else if (wide) m.outline else m.lit)
                // Tint (SRC_IN) dims the white glyph; setColorFilter is SRC_ATOP and would leave it white.
                if (Build.VERSION.SDK_INT >= 31) v.setColorStateList(m.icon, "setImageTintList", ColorStateList.valueOf(if (on) WHITE else DIM))
                else v.setInt(m.icon, "setImageAlpha", if (on) 255 else 140)
                v.setTextColor(m.text, if (on) WHITE else DIM)
                v.setContentDescription(m.tile, if (on) "${m.label}, selected" else m.label)
                v.setOnClickPendingIntent(m.tile, tap(context, m.id))
                if (!wide && Build.VERSION.SDK_INT >= 31) {
                    val size = if (on) 26f else 22f
                    v.setViewLayoutWidth(m.icon, size, TypedValue.COMPLEX_UNIT_DIP)
                    v.setViewLayoutHeight(m.icon, size, TypedValue.COMPLEX_UNIT_DIP)
                }
            }
            if (wide) {
                val m = MODES.firstOrNull { it.id == current } ?: MODES[2]
                v.setInt(R.id.hero, "setBackgroundResource", m.lit)
                v.setImageViewResource(R.id.hero_icon, m.glyph)
                v.setTextViewText(R.id.hero_title, m.title)
                v.setTextViewText(R.id.hero_line, m.line)
                val at = Prefs.modeChangedAt(context)
                v.setViewVisibility(R.id.hero_since, if (at > 0) View.VISIBLE else View.GONE)
                if (at > 0) {
                    val pattern = if (DateFormat.is24HourFormat(context)) "H:mm" else "h:mm a"
                    v.setTextViewText(R.id.hero_since, "Changed " + DateFormat.format(pattern, at))
                }
            }
            return v
        }

        private fun tap(context: Context, mode: String): PendingIntent = PendingIntent.getBroadcast(
            context, mode.hashCode(),
            Intent(context, ModeTap::class.java).setData(Uri.parse("callme://mode/$mode")),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }
}

/** Same widget, offered at 4x2 in the picker. */
class ModeWidgetWide : ModeWidget()

/** Tile taps. Not exported, so only this app's own PendingIntents can change the mode. */
class ModeTap : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val mode = intent.data?.lastPathSegment ?: return
        if (mode !in ContactMode.MODES || mode == Prefs.mode(context)) return
        Prefs.setMode(context, mode)
        ContactMode.publish(context, mode)
        ModeWidget.updateAll(context)
    }
}
