plugins {
  kotlin("jvm")
  kotlin("plugin.serialization")
}

dependencies {
  implementation("org.jetbrains.kotlinx:kotlinx-serialization-core:1.8.0")
  implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
}

kotlin {
  jvmToolchain(17)
}
