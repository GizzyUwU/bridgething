plugins {
  kotlin("jvm") version "2.3.20" apply false
  kotlin("android") version "2.3.20" apply false
  kotlin("plugin.serialization") version "2.3.20" apply false
  id("com.android.library") version "8.13.0" apply false
}

allprojects {
  group = "com.bridgething"
  version = "0.3.0"

  repositories {
    google()
    mavenCentral()
  }
}
