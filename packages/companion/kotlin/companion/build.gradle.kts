plugins {
  id("com.android.library")
  kotlin("android")
  kotlin("plugin.serialization")
}

android {
  namespace = "dev.bridgething.companion"
  compileSdk = 36

  defaultConfig {
    minSdk = 26
  }

  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }

  testOptions {
    unitTests {
      isIncludeAndroidResources = false
      isReturnDefaultValues = true
    }
  }
}

kotlin {
  jvmToolchain(21)
  compilerOptions {
    jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
  }
}

dependencies {
  api(project(":packages:gateway:kotlin:gateway"))
  api(project(":packages:glue:kotlin:glue"))
  api(project(":packages:lyrics:kotlin:lyrics"))
  api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
  api("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
  api("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
  // OkHttp drives net fetch / ws / stream + OTA artifact + manifest fetches.
  api("com.squareup.okhttp3:okhttp:4.12.0")
  api("androidx.core:core-ktx:1.13.1")
  // Play Services Location is loaded reflectively at runtime so the
  // companion still works on degoogled devices; declare as compileOnly so
  // it shows up on the classpath for IDE / type-checking but the host app
  // chooses whether to ship it.
  compileOnly("com.google.android.gms:play-services-location:21.3.0")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
