plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.awesometree.mobile"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.awesometree.mobile"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    // UniFFI-generated Kotlin bindings
    sourceSets {
        getByName("main") {
            java.srcDir("${buildDir}/generated/source/uniffi/kotlin")
        }
    }
}

dependencies {
    // Compose BOM
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)

    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.navigation:navigation-compose:2.8.5")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")

    // CameraX for QR scanning
    implementation("androidx.camera:camera-camera2:1.4.1")
    implementation("androidx.camera:camera-lifecycle:1.4.1")
    implementation("androidx.camera:camera-view:1.4.1")
    implementation("com.google.mlkit:barcode-scanning:17.3.0")

    // JNA for UniFFI bindings
    implementation("net.java.dev.jna:jna:5.15.0@aar")

    debugImplementation("androidx.compose.ui:ui-tooling")
}

// Task to generate UniFFI Kotlin bindings from the Rust .so
tasks.register<Exec>("generateUniFFIBindings") {
    val rustLib = "${rootProject.projectDir}/../target/aarch64-linux-android/release/libawesometree_core.so"
    val outDir = "${buildDir}/generated/source/uniffi/kotlin"

    workingDir = rootProject.projectDir.parentFile
    commandLine(
        "cargo", "run", "-p", "uniffi-bindgen", "--",
        "generate", "--library", rustLib,
        "--language", "kotlin",
        "--out-dir", outDir
    )

    doFirst {
        mkdir(outDir)
    }
}
