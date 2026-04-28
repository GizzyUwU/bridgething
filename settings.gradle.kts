pluginManagement {
  repositories {
    gradlePluginPortal()
    google()
    mavenCentral()
  }
}

rootProject.name = "bridgething"

include(":lib:kotlin:schema")
project(":lib:kotlin:schema").projectDir = file("lib/kotlin/schema")

include(":gateway:kotlin:gateway")
project(":gateway:kotlin:gateway").projectDir = file("gateway/kotlin/gateway")
