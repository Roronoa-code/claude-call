package fyi.mani.callme;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;

/** Opens the call screen on a test call's link, answered, without ringing (examples/phone_check.rs drives the call). */
public final class CallCheck extends Instrumentation {
    private String link;

    @Override public void onCreate(Bundle args) {
        super.onCreate(args);
        link = args == null ? null : args.getString("link");
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        if (link == null || !link.startsWith("https://")) {
            result.putString("error", "pass -e link <https call link>");
            finish(Activity.RESULT_CANCELED, result);
            return;
        }
        Intent intent = new Intent().setClassName(getTargetContext(), "fyi.mani.callme.CallActivity")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("link", link).putExtra("answer", true);
        Activity call = startActivitySync(intent);
        long until = SystemClock.elapsedRealtime() + 300_000;
        while (SystemClock.elapsedRealtime() < until && !call.isFinishing() && !call.isDestroyed()) SystemClock.sleep(250);
        result.putBoolean("closed", call.isFinishing() || call.isDestroyed());
        finish(Activity.RESULT_OK, result);
    }
}
