plugins {
  id("com.android.library")
  kotlin("android")
}

android {
  namespace = "com.bridgething.nlukit"
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

  sourceSets {
    getByName("test") { kotlin.srcDir("src/testShared/kotlin") }
    getByName("androidTest") { kotlin.srcDir("src/testShared/kotlin") }
  }
}

kotlin {
  jvmToolchain(21)
  compilerOptions {
    jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    freeCompilerArgs.add("-Xskip-metadata-version-check")
  }
}

dependencies {
  api(project(":packages:nlu:kotlin:nlu"))
  api(project(":crates:lib:kotlin:schema"))
  implementation("com.google.ai.edge.litert:litert:2.1.6")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
  androidTestImplementation("androidx.test.ext:junit:1.3.0")
  androidTestImplementation("androidx.test:runner:1.7.0")
  androidTestImplementation("androidx.test:core-ktx:1.7.0")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}
