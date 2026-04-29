pluginManagement {
  repositories {
    gradlePluginPortal()
    google()
    mavenCentral()
  }
}

rootProject.name = "bridgething"

include(":crates:lib:kotlin:schema")
project(":crates:lib:kotlin:schema").projectDir = file("crates/lib/kotlin/schema")

include(":packages:gateway:kotlin:gateway")
project(":packages:gateway:kotlin:gateway").projectDir = file("packages/gateway/kotlin/gateway")
