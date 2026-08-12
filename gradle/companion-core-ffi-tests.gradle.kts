import org.gradle.api.tasks.Exec
import org.gradle.api.tasks.testing.Test

val repoRoot = projectDir.resolve("../../../..").normalize()

val cargoBuildCompanionCore = tasks.register<Exec>("cargoBuildCompanionCore") {
  workingDir = repoRoot
  commandLine("cargo", "build", "-p", "bridgething-companion", "--lib")
}

tasks.withType<Test>().configureEach {
  dependsOn(cargoBuildCompanionCore)
  val cargoTargetDir = System.getenv("CARGO_TARGET_DIR")?.let { file(it) } ?: repoRoot.resolve("target")
  systemProperty("jna.library.path", cargoTargetDir.resolve("debug").absolutePath)
}
