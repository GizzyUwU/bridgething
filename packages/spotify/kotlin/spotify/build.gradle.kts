plugins {
  id("com.android.library")
  kotlin("android")
}

android {
  namespace = "com.bridgething.spotify"
  compileSdk = 36

  defaultConfig {
    minSdk = 26
  }

  buildFeatures {
    buildConfig = true
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
  api("net.java.dev.jna:jna:5.17.0@aar")
  api("io.ktor:ktor-client-core:3.0.0")
  api("io.ktor:ktor-client-cio:3.0.0")
  api("io.ktor:ktor-client-websockets:3.0.0")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testImplementation("io.ktor:ktor-server-cio:3.0.0")
  testImplementation("io.ktor:ktor-server-websockets:3.0.0")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
