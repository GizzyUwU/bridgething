plugins {
  id("com.android.library")
  kotlin("android")
  kotlin("plugin.serialization")
}

android {
  namespace = "dev.bridgething.spotify"
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
  api(project(":packages:glue:kotlin:glue"))
  api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
  api("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
  api("io.ktor:ktor-client-core:3.0.0")
  api("io.ktor:ktor-client-cio:3.0.0")
  api("io.ktor:ktor-client-websockets:3.0.0")
  api("io.ktor:ktor-client-content-negotiation:3.0.0")
  api("io.ktor:ktor-serialization-kotlinx-json:3.0.0")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testImplementation("io.ktor:ktor-client-mock:3.0.0")
  testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
