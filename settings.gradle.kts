pluginManagement {
  repositories {
    gradlePluginPortal()
    google()
    mavenCentral()
  }
}

plugins {
  id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

rootProject.name = "bridgething"

include(":packages:companion:kotlin:companion")
project(":packages:companion:kotlin:companion").projectDir = file("packages/companion/kotlin/companion")

include(":packages:companion:kotlin:core")
project(":packages:companion:kotlin:core").projectDir = file("packages/companion/kotlin/core")

include(":packages:asr:kotlin:whisper")
project(":packages:asr:kotlin:whisper").projectDir = file("packages/asr/kotlin/whisper")
