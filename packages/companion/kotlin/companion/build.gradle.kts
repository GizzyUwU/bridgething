plugins {
  id("com.android.library")
  kotlin("android")
  kotlin("plugin.serialization")
}

android {
  namespace = "com.bridgething.companion"
  compileSdk = 36

  defaultConfig {
    minSdk = 26
    testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
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
  api("com.squareup.okhttp3:okhttp:4.12.0")
  api("androidx.core:core-ktx:1.13.1")
  api("androidx.media:media:1.8.0")
  compileOnly("com.google.android.gms:play-services-location:21.3.0")
  testImplementation(project(":packages:spotify:kotlin:spotify"))
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testImplementation("io.mockk:mockk:1.13.13")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
  androidTestImplementation("androidx.test.ext:junit:1.2.1")
  androidTestImplementation("androidx.test:runner:1.6.2")
  androidTestImplementation("androidx.test:core-ktx:1.6.1")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
