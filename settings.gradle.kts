rootProject.name = "bridgething"

include(":lib:kotlin:schema")
project(":lib:kotlin:schema").projectDir = file("lib/kotlin/schema")

include(":lib:kotlin:gateway")
project(":lib:kotlin:gateway").projectDir = file("lib/kotlin/gateway")
