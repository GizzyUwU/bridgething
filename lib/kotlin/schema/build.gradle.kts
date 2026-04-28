plugins {
  kotlin("jvm")
  kotlin("plugin.serialization")
}

dependencies {
  api("org.jetbrains.kotlinx:kotlinx-serialization-core:1.8.0")
  api("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
  // kotlin-reflect drives AdjacentTaggedSerializer's sealed-subclass discovery
  // so adding a new variant in Rust + regenerating Kotlin is the only change
  // needed — no per-class serializer edit.
  implementation(kotlin("reflect"))
  // UniversalValueSerializer delegates to MsgPackNullableDynamicSerializer for
  // the binary path so JsonElement payloads round-trip over msgpack.
  api("com.ensarsarajcic.kotlinx:serialization-msgpack:0.6.1")
}

kotlin {
  // Bumped to 26 because msgpack 0.6.1 is compiled at class file 65 (JVM 21+).
  // Output bytecode stays at 17 for consumer compat — Android R8 desugars.
  jvmToolchain(26)
  compilerOptions {
    jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
  }
}

java {
  sourceCompatibility = JavaVersion.VERSION_17
  targetCompatibility = JavaVersion.VERSION_17
}
