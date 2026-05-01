plugins {
  id("com.android.library")
  kotlin("android")
  kotlin("plugin.serialization")
}

android {
  namespace = "dev.bridgething.gateway"
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
  jvmToolchain(26)
  compilerOptions {
    jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
  }
}

dependencies {
  api(project(":crates:lib:kotlin:schema"))
  implementation("com.ensarsarajcic.kotlinx:serialization-msgpack:0.6.1")
  api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
  api("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
  testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
  testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
