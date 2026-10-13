plugins {
    id("com.android.application")
}

android {
    namespace = "fyi.mani.callme"
    compileSdk = 37
    buildToolsVersion = "36.0.0"

    defaultConfig {
        applicationId = "fyi.mani.callme"
        minSdk = 29
        targetSdk = 37
        versionCode = 1
        versionName = "1.0.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Personal sideload build: signed with this PC's debug key so updates install over it.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

// The call screen is the Rust library in phone/: built for the phone (arm64) before every app build.
val rustLib = tasks.register<Exec>("rustLib") {
    workingDir = rootProject.file("..")
    // Slint finds the Android platform jar itself, but not with this SDK's "android-37.0" folder names.
    environment("ANDROID_JAR", System.getenv("ANDROID_HOME") + "/platforms/android-36/android.jar")
    commandLine(System.getProperty("user.home") + "/.cargo/bin/cargo", "ndk", "-t", "arm64-v8a", "-P", "29",
        "-o", "android/app/src/main/jniLibs", "build", "--release", "-p", "callme-phone")
}
tasks.named("preBuild") { dependsOn(rustLib) }
