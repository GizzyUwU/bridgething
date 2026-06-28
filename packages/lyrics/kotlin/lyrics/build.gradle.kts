plugins {
  id("com.android.library")
  kotlin("android")
  kotlin("plugin.serialization")
}

android {
  namespace = "com.bridgething.lyrics"
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
  api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
  api("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
  implementation("io.ktor:ktor-client-core:3.0.0")
  implementation("io.ktor:ktor-client-cio:3.0.0")
  implementation("io.ktor:ktor-client-content-negotiation:3.0.0")
  implementation("io.ktor:ktor-serialization-kotlinx-json:3.0.0")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
  testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
