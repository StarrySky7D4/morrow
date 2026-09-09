import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val releaseKey = Properties()
val releaseKeyFile = rootProject.file("key.properties")
if (releaseKeyFile.exists()) releaseKeyFile.inputStream().use { releaseKey.load(it) }

android {
    namespace = "dev.morrow.morrow_studio"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // Stable installed identity preserves upgrades and app data after rebranding.
        applicationId = "dev.starrysky7d4.daemon"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (releaseKeyFile.exists()) {
            create("release") {
                storeFile = rootProject.file(releaseKey.getProperty("storeFile"))
                storePassword = releaseKey.getProperty("storePassword")
                keyAlias = releaseKey.getProperty("keyAlias")
                keyPassword = releaseKey.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".preview"
            versionNameSuffix = "-preview"
        }
        release {
            if (releaseKeyFile.exists()) signingConfig = signingConfigs.getByName("release")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

// Never silently distribute a release APK signed with a disposable debug key.
gradle.taskGraph.whenReady {
    if (allTasks.any { it.project == project && it.name.contains("Release") } &&
        !releaseKeyFile.exists()) {
        throw GradleException("Release requires android/key.properties. Use flutter build apk --debug for a preview.")
    }
}

dependencies {
    implementation("androidx.core:core:1.15.0")
}

flutter {
    source = "../.."
}
