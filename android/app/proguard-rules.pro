# Called from the Rust call screen through JNI.
-keep class fyi.mani.callme.CallActivity { public void callBegan(); }
# The Rust side calls these by name.
-keep class fyi.mani.callme.Native { *; }
-keep class fyi.mani.callme.MainActivity { public *; }
