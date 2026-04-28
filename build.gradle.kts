plugins {
  kotlin("jvm") version "2.3.20" apply false
  kotlin("plugin.serialization") version "2.3.20" apply false
}

allprojects {
  group = "dev.bridgething"
  version = "0.1.0"

  repositories {
    mavenCentral()
  }
}
