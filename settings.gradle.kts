pluginManagement {
  repositories {
    gradlePluginPortal()
    google()
    mavenCentral()
  }
}

plugins {
  // Auto-downloads JDK toolchains the kotlin modules declare via
  // jvmToolchain(...) when the host machine doesn't have a matching JDK.
  id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

rootProject.name = "bridgething"

include(":crates:lib:kotlin:schema")
project(":crates:lib:kotlin:schema").projectDir = file("crates/lib/kotlin/schema")

include(":packages:gateway:kotlin:gateway")
project(":packages:gateway:kotlin:gateway").projectDir = file("packages/gateway/kotlin/gateway")

include(":packages:glue:kotlin:glue")
project(":packages:glue:kotlin:glue").projectDir = file("packages/glue/kotlin/glue")

include(":packages:spotify:kotlin:spotify")
project(":packages:spotify:kotlin:spotify").projectDir = file("packages/spotify/kotlin/spotify")

include(":packages:apple-music:kotlin:apple-music")
project(":packages:apple-music:kotlin:apple-music").projectDir = file("packages/apple-music/kotlin/apple-music")

include(":packages:tidal:kotlin:tidal")
project(":packages:tidal:kotlin:tidal").projectDir = file("packages/tidal/kotlin/tidal")

include(":packages:lyrics:kotlin:lyrics")
project(":packages:lyrics:kotlin:lyrics").projectDir = file("packages/lyrics/kotlin/lyrics")

include(":packages:companion:kotlin:companion")
project(":packages:companion:kotlin:companion").projectDir = file("packages/companion/kotlin/companion")
