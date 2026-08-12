plugins {
  id("com.android.library")
  kotlin("android")
}

android {
  namespace = "com.bridgething.companion.core"
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
  api("net.java.dev.jna:jna:5.17.0@aar")
  api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
  testImplementation("net.java.dev.jna:jna:5.17.0")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<Test>().configureEach {
  useJUnitPlatform()
}

apply(from = "$projectDir/../../../../gradle/companion-core-ffi-tests.gradle.kts")

val cargoNdkAbis = (project.findProperty("cargoNdkAbis") as String?)?.split(',')?.filter { it.isNotBlank() }
if (!cargoNdkAbis.isNullOrEmpty()) {
  val repoRoot = projectDir.resolve("../../../..").normalize()
  val coreLibrary = "libbridgething_companion.so"
  val androidTriples = mapOf(
    "arm64-v8a" to "aarch64-linux-android",
    "armeabi-v7a" to "armv7-linux-androideabi",
    "x86" to "i686-linux-android",
    "x86_64" to "x86_64-linux-android",
  )

  val installCargoNdk = tasks.register<Exec>("installCargoNdk") {
    workingDir = repoRoot
    commandLine("bash", "-c", "cargo ndk --version >/dev/null 2>&1 || cargo install cargo-ndk --locked")
  }

  val cargoNdkCompanionCore = tasks.register<Exec>("cargoNdkCompanionCore") {
    dependsOn(installCargoNdk)
    workingDir = repoRoot
    environment("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
    val targets = cargoNdkAbis.joinToString(" ") { "-t $it" }
    val jniLibs = projectDir.resolve("src/main/jniLibs").path
    val copies = cargoNdkAbis.joinToString(" ") { abi ->
      val triple = androidTriples[abi] ?: error("unknown android abi '$abi'")
      "rm -rf '$jniLibs/$abi'; mkdir -p '$jniLibs/$abi'; " +
        "cp \"\$target/$triple/release/$coreLibrary\" '$jniLibs/$abi/$coreLibrary';"
    }
    commandLine(
      "bash",
      "-c",
      "set -euo pipefail; " +
        "if [ -z \"\${ANDROID_NDK_HOME:-}\" ]; then " +
        "sdk=\"\${ANDROID_HOME:-\${ANDROID_SDK_ROOT:?no android sdk in the environment}}\"; " +
        "export ANDROID_NDK_HOME=\"\$(ls -d \"\$sdk\"/ndk/* | sort -V | tail -1)\"; fi; " +
        "export ANDROID_NDK_ROOT=\"\$ANDROID_NDK_HOME\"; " +
        "cargo ndk $targets build --release --no-default-features -p bridgething-companion --lib; " +
        "target=\"\${CARGO_TARGET_DIR:-${repoRoot.resolve("target").path}}\"; " +
        copies,
    )
  }

  tasks.matching { it.name == "preBuild" }.configureEach { dependsOn(cargoNdkCompanionCore) }
}
