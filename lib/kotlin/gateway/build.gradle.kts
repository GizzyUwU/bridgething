plugins {
  kotlin("jvm")
  kotlin("plugin.serialization")
}

dependencies {
  api(project(":lib:kotlin:schema"))
  // kotlinx-serialization-msgpack 0.6.x targets JVM 21+ class files at runtime;
  // gzip uses java.util.zip from the JDK so no extra dep there.
  implementation("com.ensarsarajcic.kotlinx:serialization-msgpack:0.6.1")
  testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
  testRuntimeOnly("org.junit.platform:junit-platform-launcher")
  testImplementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
}

kotlin {
  // 21 isn't installed locally; 26 is. Bytecode target 17 keeps consumer-side
  // compat — Android consumers pull msgpack 0.6.1 at class file 65 and rely on
  // R8 desugaring on older runtimes regardless of what we output here.
  jvmToolchain(26)
  compilerOptions {
    jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
  }
}

java {
  sourceCompatibility = JavaVersion.VERSION_17
  targetCompatibility = JavaVersion.VERSION_17
}

tasks.test {
  useJUnitPlatform()
}
