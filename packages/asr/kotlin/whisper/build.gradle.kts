plugins {
  id("com.android.library")
  kotlin("android")
}

val testAssetDir = "/data/local/tmp/bridgething-asr"

val modelPath = providers.environmentVariable("BRIDGETHING_WHISPER_MODEL")
val fixturePath = providers.environmentVariable("BRIDGETHING_WHISPER_FIXTURE")

android {
  namespace = "com.bridgething.asr.whisper"
  compileSdk = 36
  ndkVersion = "27.1.12297006"

  defaultConfig {
    minSdk = 26
    testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

    ndk { abiFilters += "arm64-v8a" }

    externalNativeBuild {
      cmake {
        arguments += "-DANDROID_STL=c++_static"
        arguments += "-DFETCHCONTENT_BASE_DIR=${layout.buildDirectory.get().asFile}/whisper-src"
      }
    }

    if (modelPath.isPresent) {
      testInstrumentationRunnerArguments["whisperModel"] = "$testAssetDir/${file(modelPath.get()).name}"
    }
    if (fixturePath.isPresent) {
      testInstrumentationRunnerArguments["whisperFixture"] = "$testAssetDir/${file(fixturePath.get()).name}"
    }
  }

  externalNativeBuild {
    cmake {
      path = file("src/main/cpp/CMakeLists.txt")
      version = "3.22.1"
    }
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
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
  androidTestImplementation("androidx.test.ext:junit:1.2.1")
  androidTestImplementation("androidx.test:runner:1.6.2")
  androidTestImplementation("androidx.test:core-ktx:1.6.1")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}

val pushWhisperTestAssets by
  tasks.registering(Exec::class) {
    description = "Stages the whisper model and audio fixture on the connected device."
    onlyIf { modelPath.isPresent && fixturePath.isPresent }

    val adb = android.sdkDirectory.resolve("platform-tools/adb").absolutePath
    commandLine(
      "sh",
      "-c",
      """
      set -e
      "$adb" shell mkdir -p $testAssetDir
      "$adb" push "${modelPath.orNull}" $testAssetDir/
      "$adb" push "${fixturePath.orNull}" $testAssetDir/
      "$adb" shell chmod 755 $testAssetDir
      "$adb" shell chmod 644 $testAssetDir/*
      """
        .trimIndent(),
    )
  }

tasks.matching { it.name == "connectedDebugAndroidTest" }.configureEach { dependsOn(pushWhisperTestAssets) }
