# The Rust library's entry points (phone/src/shell.rs) are found by name.
-keep class fyi.mani.callme.Native { *; }
# ...and what Rust reads or calls on the activities (live_gl.rs, home.rs, inbox.rs: Shown in Shell.kt).
-keep class fyi.mani.callme.Shown { public *; }
