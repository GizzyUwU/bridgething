use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
  process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use syn::{Item, Meta};

mod defaults;
mod dispatch;

const TS_BINDINGS_DIR: &str = "crates/lib/ts/bindings";
const TS_UUID_FIELDS_OUTPUT: &str = "crates/lib/ts/uuid-fields.generated.ts";
const SWIFT_OUTPUT: &str = "crates/lib/swift/Sources/BridgethingSchema/Generated.swift";
const KOTLIN_OUTPUT: &str = "crates/lib/kotlin/schema/src/main/kotlin/com/bridgething/schema/Generated.kt";
const KOTLIN_PACKAGE: &str = "com.bridgething.schema";
const LIB_SRC: &str = "crates/lib/src";
const RUST_CLIENT_OUTPUT: &str = "crates/client-rs/src/surface.generated.rs";
const RUST_GATEWAY_OUTPUT: &str = "crates/gateway-rs/src/surface.generated.rs";
const DOCS_OUTPUT: &str = "crates/lib/docs/surfaces.json";

fn workspace_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("../..")
    .canonicalize()
    .expect("workspace root resolves")
}

fn main() -> Result<()> {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let target = args.first().map(String::as_str).unwrap_or("all");

  std::env::set_current_dir(workspace_root()).context("cd to workspace root")?;

  match target {
    "ts" | "typescript" => gen_typescript()?,
    "swift" => gen_swift()?,
    "kotlin" => gen_kotlin()?,
    "rust" => gen_rust()?,
    "manifest" => gen_manifests()?,
    "docs" => gen_docs()?,
    "all" => {
      gen_typescript()?;
      gen_swift()?;
      gen_kotlin()?;
      gen_rust()?;
      gen_docs()?;
    }
    other => bail!("unknown target {other:?}; expected one of: ts, swift, kotlin, rust, manifest, docs, all"),
  }
  Ok(())
}

/// Emit only the wire-surface coverage manifests (Swift + Kotlin) without
/// the typeshare DTO regeneration, so they can be refreshed on hosts that
/// lack the typeshare CLI. The canonical `swift`/`kotlin`/`all` targets also
/// emit these as part of their flow.
fn gen_manifests() -> Result<()> {
  println!("==> manifests");
  let inv = dispatch::inventory(LIB_SRC).context("dispatch inventory")?;
  let plan = dispatch::build_plan_for(&inv, dispatch::Protocol::Gateway).context("dispatch plan")?;
  dispatch::emit_swift_manifest(&plan).context("emit swift wire-surface manifest")?;
  dispatch::emit_kotlin_manifest(&plan).context("emit kotlin wire-surface manifest")?;
  println!("    emitted Swift + Kotlin WireSurfaceManifest");
  Ok(())
}

/// Emit the client SDK reference IR (`crates/lib/docs/surfaces.json`)
/// that bridgething.com renders at `/docs`. Client protocol only - the
/// webapp-facing surface. Committed and refreshed by `just codegen`.
fn gen_docs() -> Result<()> {
  println!("==> docs");
  let inv = dispatch::inventory(LIB_SRC).context("dispatch inventory")?;
  let plan = dispatch::build_plan_for(&inv, dispatch::Protocol::Client).context("dispatch plan")?;
  dispatch::emit_docs_json(&plan, &inv, DOCS_OUTPUT).context("emit docs json")?;
  println!("    emitted {DOCS_OUTPUT}");
  Ok(())
}

/// Emit the Rust SDK surface files (`bridgething-client` /
/// `bridgething-gateway`). Pure naming sugar over the generic runtime;
/// no DTOs are materialized (the types are native Rust in `libbridgething`).
fn gen_rust() -> Result<()> {
  println!("==> rust");
  let inv = dispatch::inventory(LIB_SRC).context("dispatch inventory")?;
  let plans = dispatch::build_plans(&inv).context("dispatch plans")?;
  for plan in &plans {
    let target = match plan.protocol {
      dispatch::Protocol::Client => dispatch::RustTarget {
        out_path: RUST_CLIENT_OUTPUT,
        sdk_type: "Client",
        wire_mod: "client",
      },
      dispatch::Protocol::Gateway => dispatch::RustTarget {
        out_path: RUST_GATEWAY_OUTPUT,
        sdk_type: "Gateway",
        wire_mod: "gateway",
      },
    };
    dispatch::emit_rust(plan, &target).with_context(|| format!("emit rust dispatch for {:?}", plan.protocol))?;
    println!("    emitted {}", target.out_path);
  }
  run(
    "rustfmt",
    &["--edition", "2024", RUST_CLIENT_OUTPUT, RUST_GATEWAY_OUTPUT],
  )?;
  Ok(())
}

fn gen_typescript() -> Result<()> {
  println!("==> typescript");
  if Path::new(TS_BINDINGS_DIR).exists() {
    std::fs::remove_dir_all(TS_BINDINGS_DIR).context("clear ts/bindings")?;
  }
  run("cargo", &["test", "-p", "libbridgething", "--quiet"])?;
  run("bunx", &["prettier", TS_BINDINGS_DIR, "--write", "--log-level", "warn"])?;
  add_ts_import_extensions(TS_BINDINGS_DIR).context("add .js extensions to ts bindings")?;

  println!("    emitting ts dispatch helpers");
  let inv = dispatch::inventory(LIB_SRC).context("dispatch inventory")?;
  println!(
    "    dispatch inventory: {} marker-tagged types, {} typed-request decls, {} uuid fields",
    inv.markers.len(),
    inv.typed_requests.len(),
    inv.uuid_field_names.len(),
  );
  let plans = dispatch::build_plans(&inv).context("dispatch plans")?;
  let binding_locations = scan_binding_type_locations(TS_BINDINGS_DIR).context("scan ts bindings")?;
  for plan in &plans {
    match plan.protocol {
      dispatch::Protocol::Gateway => {
        dispatch::emit_typescript(plan, &binding_locations).context("emit gateway typescript dispatch")?
      }
      dispatch::Protocol::Client => {
        dispatch::emit_typescript_client(plan, &binding_locations).context("emit client typescript dispatch")?
      }
    }
  }
  emit_ts_uuid_fields(&inv.uuid_field_names).context("emit ts uuid fields")?;
  run(
    "bunx",
    &[
      "prettier",
      "packages/gateway/typescript/src/dispatch.generated.ts",
      "packages/client-ts/src/dispatch.generated.ts",
      TS_UUID_FIELDS_OUTPUT,
      "--write",
      "--log-level",
      "warn",
    ],
  )?;
  Ok(())
}

/// ts-rs emits extensionless relative specifiers, which node's ESM resolver
/// rejects once the package is published. Rewrite them to explicit `.js`.
fn add_ts_import_extensions(dir: &str) -> Result<()> {
  let specifier = regex::Regex::new(r#"(from\s+|import\s*\(\s*)(['"])(\.{1,2}/[^'"]+)(['"])"#)?;
  for entry in std::fs::read_dir(dir).with_context(|| format!("read {dir}"))? {
    let path = entry?.path();
    if path.extension().and_then(|e| e.to_str()) != Some("ts") {
      continue;
    }
    let input = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let output = specifier.replace_all(&input, |caps: &regex::Captures| {
      let spec = &caps[3];
      let suffix = if spec.ends_with(".js") { "" } else { ".js" };
      format!("{}{}{}{}{}", &caps[1], &caps[2], spec, suffix, &caps[4])
    });
    if output != input {
      std::fs::write(&path, output.as_ref()).with_context(|| format!("write {}", path.display()))?;
    }
  }
  Ok(())
}

/// Scan `ts/bindings/*.ts` and build a `name -> {files}` map (`AssetGet
/// -> {"client.ts"}`, `Priority -> {"shared.ts"}`, `WebappError ->
/// {"client.ts", "gateway.ts"}` for the genuinely-distinct case). The
/// dispatch emitters consult this to route per-type imports into the
/// right `@bridgething/lib` subpath, preferring the protocol-matching
/// file when a name is defined in both client.ts and gateway.ts.
fn scan_binding_type_locations(dir: &str) -> Result<BTreeMap<String, BTreeSet<String>>> {
  let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
  for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {dir}"))? {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|s| s.to_str()) != Some("ts") {
      continue;
    }
    let file_name = path
      .file_name()
      .and_then(|s| s.to_str())
      .unwrap_or_default()
      .to_string();
    let content = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    for line in content.lines() {
      let trimmed = line.trim_start();
      let prefix = "export type ";
      if let Some(rest) = trimmed.strip_prefix(prefix) {
        let name: String = rest
          .chars()
          .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
          .collect();
        if !name.is_empty() {
          map.entry(name).or_default().insert(file_name.clone());
        }
      }
    }
  }
  Ok(map)
}

fn emit_ts_uuid_fields(names: &BTreeSet<String>) -> Result<()> {
  let mut out = String::new();
  out.push_str("// @generated by tools/codegen - do not edit. Run `just codegen` to regenerate.\n\n");
  out.push_str("/**\n");
  out.push_str(" * Field names whose Rust type is `Uuid`. Both wire codecs walk decoded\n");
  out.push_str(" * payloads and convert at these keys: msgpack 16-byte `bin` ↔ uuid string\n");
  out.push_str(" * on the gateway path; the daemon already speaks uuid strings on JSON.\n");
  out.push_str(" */\n");
  out.push_str("export const UUID_FIELD_NAMES: ReadonlySet<string> = new Set([\n");
  for name in names {
    out.push_str(&format!("  '{name}',\n"));
  }
  out.push_str("]);\n");
  std::fs::write(TS_UUID_FIELDS_OUTPUT, out).context("write uuid-fields.generated.ts")?;
  Ok(())
}

fn gen_swift() -> Result<()> {
  println!("==> swift");
  run("typeshare", &["--lang=swift", "--output-file", SWIFT_OUTPUT, LIB_SRC])?;

  let inv = dispatch::inventory(LIB_SRC).context("dispatch inventory")?;
  let wire_defaults = defaults::discover(LIB_SRC).context("resolve #[serde(default)] wire defaults")?;
  println!(
    "    resolved {} defaulted field(s) across {} type(s)",
    wire_defaults.field_count(),
    wire_defaults.by_type.len()
  );
  let content = std::fs::read_to_string(SWIFT_OUTPUT).context("read swift output")?;
  let patched = patch_swift(&content, &inv.uuid_field_names, &wire_defaults)?;
  std::fs::write(SWIFT_OUTPUT, patched).context("write swift output")?;

  println!("    emitting swift dispatch helpers");
  let plan = dispatch::build_plan_for(&inv, dispatch::Protocol::Gateway).context("dispatch plan")?;
  dispatch::emit_swift(&plan).context("emit swift dispatch")?;
  dispatch::emit_swift_manifest(&plan).context("emit swift wire-surface manifest")?;
  Ok(())
}

fn gen_kotlin() -> Result<()> {
  println!("==> kotlin");
  run(
    "typeshare",
    &[
      "--lang=kotlin",
      &format!("--java-package={KOTLIN_PACKAGE}"),
      "--output-file",
      KOTLIN_OUTPUT,
      LIB_SRC,
    ],
  )?;

  let adjacent_tagged =
    discover_adjacent_tagged_enums(LIB_SRC).context("discover adjacent-tagged enums in crates/lib/src")?;
  println!(
    "    discovered {} adjacent-tagged enum(s): {}",
    adjacent_tagged.len(),
    adjacent_tagged
      .iter()
      .map(|e| e.name.as_str())
      .collect::<Vec<_>>()
      .join(", ")
  );

  let inv = dispatch::inventory(LIB_SRC).context("dispatch inventory")?;
  let wire_defaults = defaults::discover(LIB_SRC).context("resolve #[serde(default)] wire defaults")?;
  println!(
    "    resolved {} defaulted field(s) across {} type(s)",
    wire_defaults.field_count(),
    wire_defaults.by_type.len()
  );
  let content = std::fs::read_to_string(KOTLIN_OUTPUT).context("read kotlin output")?;
  let patched = patch_kotlin(&content, &adjacent_tagged, &inv.uuid_field_names, &wire_defaults)?;
  std::fs::write(KOTLIN_OUTPUT, patched).context("write kotlin output")?;

  emit_kotlin_serializers(&adjacent_tagged).context("emit kotlin serializers")?;

  println!("    emitting kotlin dispatch helpers");
  let plan = dispatch::build_plan_for(&inv, dispatch::Protocol::Gateway).context("dispatch plan")?;
  dispatch::emit_kotlin(&plan).context("emit kotlin dispatch")?;
  dispatch::emit_kotlin_manifest(&plan).context("emit kotlin wire-surface manifest")?;
  Ok(())
}

fn emit_kotlin_serializers(enums: &[AdjacentTaggedEnum]) -> Result<()> {
  let path = "crates/lib/kotlin/schema/src/main/kotlin/com/bridgething/schema/Serializers.kt";
  let mut out = String::new();
  out.push_str("// @generated by tools/codegen - do not edit. Run `just codegen` to regenerate.\n");
  out.push_str("package com.bridgething.schema\n\n");
  out.push_str("import kotlinx.serialization.KSerializer\n\n");
  out.push_str(
    "/**\n * Per-class adjacent-tagged serializers, one per multi-discriminator sealed\n * class in the schema. `patch_kotlin` annotates each affected sealed class\n * with `@Serializable(with = ...)` so kotlinx-serialization picks these up\n * regardless of format (msgpack, json, ...).\n */\n",
  );
  for e in enums {
    out.push_str(&format!(
      "\npublic object {name}Serializer :\n  KSerializer<{name}> by AdjacentTaggedSerializer({name}::class, discriminator = \"{tag}\")\n",
      name = e.name,
      tag = e.tag,
    ));
  }
  std::fs::write(path, out).with_context(|| format!("write {path}"))?;
  Ok(())
}

fn patch_swift(
  input: &str,
  uuid_field_names: &BTreeSet<String>,
  wire_defaults: &defaults::DefaultsIndex,
) -> Result<String> {
  // typeshare emits one definition per Rust struct, even when two
  // structs in different modules share an identical name and body
  // (deliberate when a shared payload is wired to two surfaces). Drop
  // duplicates with matching bodies; bail if bodies diverge (real type
  // mismatch worth surfacing).
  let deduped = dedup_swift_decls(input)?;
  // typeshare emits `[UInt8]` for `Vec<u8>`. Swift's Codable plus every
  // msgpack lib distinguishes Data (encodes as msgpack bin) from
  // [UInt8] (encodes as an array of int). Our wire is bin.
  let mut out = deduped.replace("[UInt8]", "Data");
  // Generated structs travel through actor-isolated stream events on
  // the gateway side; Swift 6 strict concurrency requires Sendable.
  // Every typeshare-emitted type is a value type whose stored fields
  // are already Sendable, so adding the conformance blanket is safe.
  out = out.replace(": Codable {", ": Codable, Sendable {");
  out = out.replace(": String, Codable {", ": String, Codable, Sendable {");

  // Surface UUID-typed fields as `Foundation.UUID` instead of raw
  // `Data`, with `@MsgpackUuid` bridging the 16-byte msgpack bin via a
  // Codable property wrapper. Field names come from the codegen-emitted
  // set; only `Uuid`-Rust-typed fields are rewritten. Optional UUID
  // fields (Rust `Option<Uuid>`) become `UUID?` and use a sibling
  // `@OptionalMsgpackUuid` wrapper so the underlying `Data?` shape on
  // the wire decodes correctly.
  for name in uuid_field_names {
    let n = regex::escape(name);
    let param = regex::Regex::new(&format!(r"\b{n}: Data\b")).expect("uuid param regex");
    out = param.replace_all(&out, format!("{name}: UUID")).into_owned();
    let optional_field =
      regex::Regex::new(&format!(r"(?m)^\tpublic let {n}: UUID\?$")).expect("uuid optional field regex");
    out = optional_field
      .replace_all(&out, format!("\t@OptionalMsgpackUuid public var {name}: UUID?"))
      .into_owned();
    let field = regex::Regex::new(&format!(r"(?m)^\tpublic let {n}: UUID$")).expect("uuid field regex");
    out = field
      .replace_all(&out, format!("\t@MsgpackUuid public var {name}: UUID"))
      .into_owned();
  }

  // Runs last so the emitted provider types see the final (uuid- and
  // Data-rewritten) Swift type of each field.
  out = apply_swift_defaults(&out, wire_defaults)?;

  Ok(out)
}

/// Wrap every `#[serde(default)]` field in `@WireDefault<...>` and emit the
/// provider type each wrapper reads its value from.
///
/// typeshare renders a defaulted field as optional; the wrapper lets it go
/// back to non-optional, which is what serde actually guarantees, so the
/// trailing `?` is dropped from both the stored property and the memberwise
/// initializer.
///
/// A field the index knows about but that is present-and-unpatched in the
/// output is a hard error: silently leaving it required is the exact failure
/// this pass exists to prevent.
fn apply_swift_defaults(input: &str, wire_defaults: &defaults::DefaultsIndex) -> Result<String> {
  let struct_header = regex::Regex::new(r"^public struct (\w+):").expect("swift struct header regex");
  let field_line = regex::Regex::new(r"^\tpublic let (\w+): (.+)$").expect("swift field regex");

  let mut out = String::with_capacity(input.len());
  let mut providers: Vec<String> = Vec::new();
  let mut applied: BTreeSet<(String, String)> = BTreeSet::new();
  let mut current: Option<String> = None;
  // Parameter rewrites owed to the current struct's memberwise init.
  let mut pending_params: Vec<(String, String)> = Vec::new();

  for line in input.split_inclusive('\n') {
    let body = line.trim_end_matches('\n');
    if let Some(cap) = struct_header.captures(body) {
      current = Some(cap[1].to_string());
      pending_params.clear();
    } else if body == "}" {
      current = None;
      pending_params.clear();
    }

    if body.starts_with("\tpublic init(") && !pending_params.is_empty() {
      let mut init_line = line.to_string();
      for (field, swift_ty) in &pending_params {
        init_line = init_line.replace(&format!("{field}: {swift_ty}?"), &format!("{field}: {swift_ty}"));
      }
      out.push_str(&init_line);
      continue;
    }

    let patched = current.as_ref().and_then(|ty| {
      let cap = field_line.captures(body)?;
      let field = cap[1].to_string();
      let value = wire_defaults
        .get(ty)?
        .iter()
        .find(|candidate| candidate.field == field)?;
      let swift_ty = cap[2].trim_end_matches('?').to_string();
      let provider = format!("WireDefault{ty}{}", defaults::pascal(&field));
      providers.push(format!(
        "public enum {provider}: WireDefaultProvider {{\n\tpublic static var wireDefault: {swift_ty} {{ {} }}\n}}\n",
        defaults::swift_expr(&value.value)
      ));
      applied.insert((ty.clone(), field.clone()));
      pending_params.push((field.clone(), swift_ty.clone()));
      Some(format!(
        "\t@WireDefault<{provider}> public var {field}: {swift_ty}\n"
      ))
    });

    out.push_str(patched.as_deref().unwrap_or(line));
  }

  verify_defaults_applied(input, wire_defaults, &applied, |ty| {
    format!("public struct {ty}:")
  })?;

  if !providers.is_empty() {
    out.push_str("\n// MARK: - wire defaults\n\n");
    out.push_str(&providers.join("\n"));
  }
  Ok(out)
}

/// Cross-check the patch against the index. Types typeshare never emitted are
/// a warning; a type that IS in the output with an unpatched defaulted field
/// is a build failure.
fn verify_defaults_applied(
  output: &str,
  wire_defaults: &defaults::DefaultsIndex,
  applied: &BTreeSet<(String, String)>,
  header_for: impl Fn(&str) -> String,
) -> Result<()> {
  let mut missing: Vec<String> = Vec::new();
  for (ty, fields) in &wire_defaults.by_type {
    if !output.contains(&header_for(ty)) {
      eprintln!("    warning: `{ty}` has defaulted fields but was not emitted by typeshare");
      continue;
    }
    for field in fields {
      if !applied.contains(&(ty.clone(), field.field.clone())) {
        missing.push(format!("{ty}.{}", field.field));
      }
    }
  }
  if !missing.is_empty() {
    bail!(
      "these `#[serde(default)]` fields were emitted as required keys and would break cross-version decode: {}",
      missing.join(", ")
    );
  }
  Ok(())
}

fn patch_kotlin_uuid_imports(input: &str) -> String {
  let needle = "import kotlinx.serialization.SerialName\n";
  if !input.contains(needle) || input.contains("import java.util.UUID\n") {
    return input.to_string();
  }
  let inserted = format!("{needle}import java.util.UUID\n");
  input.replacen(needle, &inserted, 1)
}

fn patch_kotlin(
  input: &str,
  adjacent_tagged: &[AdjacentTaggedEnum],
  uuid_field_names: &BTreeSet<String>,
  wire_defaults: &defaults::DefaultsIndex,
) -> Result<String> {
  // typeshare emits one definition per Rust struct, even when two
  // structs in different modules share an identical name and body
  // (deliberate when a shared payload is wired to two surfaces). Drop
  // duplicates with matching bodies; bail if bodies diverge (real type
  // mismatch worth surfacing).
  let deduped = dedup_kotlin_decls(input)?;
  // typeshare emits `List<UByte>` for `Vec<u8>`. kotlinx-msgpack
  // encodes ByteArray as msgpack bin, but List<UByte> as an array of
  // ints. Our wire is bin.
  let mut out = deduped.replace("List<UByte>", "ByteArray");

  // Surface UUID-typed fields as `java.util.UUID` instead of raw
  // `ByteArray`, with `MsgpackUuidSerializer` bridging the 16-byte
  // msgpack bin. Field names come from the codegen-emitted set; only
  // those with `Uuid` Rust type are rewritten.
  out = patch_kotlin_uuid_imports(&out);
  let uuid_field = regex::Regex::new(r"(?m)^(\t)val (\w+): ByteArray(,?)$")?;
  out = uuid_field
    .replace_all(&out, |caps: &regex::Captures| {
      let indent = &caps[1];
      let name = &caps[2];
      let comma = &caps[3];
      if uuid_field_names.contains(name) {
        format!("{indent}@Serializable(with = MsgpackUuidSerializer::class) val {name}: UUID{comma}")
      } else {
        format!("{indent}val {name}: ByteArray{comma}")
      }
    })
    .into_owned();

  // For every adjacent-tagged enum (`#[serde(tag, content)]`), typeshare
  // emits a sealed class. kotlinx's default polymorphism for binary
  // formats sibling-inlines the discriminator into the parent map; the
  // bridgething wire shape is a nested `{<disc>: "tag", "data": payload}`
  // object. The `XSerializer` proxies in Serializers.kt produce that
  // shape, but only when the sealed class is annotated to use them.
  for e in adjacent_tagged {
    let name = &e.name;
    let pattern = regex::Regex::new(&format!(r"(?m)^@Serializable\nsealed class {}\b", regex::escape(name)))?;
    let replacement = format!("@Serializable(with = {name}Serializer::class)\nsealed class {name}");
    let new_out = pattern.replace_all(&out, replacement.as_str()).into_owned();
    if new_out == out {
      eprintln!(
        "    warning: adjacent-tagged enum {name} found in crates/lib/src but not in kotlin output (unreachable from any #[typeshare]'d root, or typeshare's emitted shape changed)"
      );
    }
    out = new_out;
  }

  // ForwardMessage::Json carries an arbitrary `serde_json::Value`.
  // kotlinx's default JsonElement serializer only works with
  // JsonDecoder. UniversalValueSerializer dispatches on encoder/decoder
  // type so the variant round-trips over msgpack as well.
  let json_pattern = "data class Json(val data: Value)";
  let json_replacement = "data class Json(@Serializable(with = UniversalValueSerializer::class) val data: Value)";
  if !out.contains(json_pattern) {
    return Err(anyhow!(
      "kotlin output is missing expected `{json_pattern}`; ForwardMessage shape may have changed"
    ));
  }
  out = out.replace(json_pattern, json_replacement);

  // Kotlin scopes the variant's name above its sibling sealed-class
  // peers, so `data class WebappError(val data: WebappError)` makes the
  // inner type recursively resolve to the variant itself instead of the
  // package-level `WebappError` sealed class. Fully-qualify any
  // self-shadowing variant payload so it points at the outer type.
  // Match `data class X(val data: Y):` and rewrite when X == Y.
  let self_shadow = regex::Regex::new(r"(?m)^(\tdata class (\w+)\(val data: )(\w+)(\): )")?;
  out = self_shadow
    .replace_all(&out, |caps: &regex::Captures| {
      let prefix = &caps[1];
      let variant = &caps[2];
      let payload = &caps[3];
      let suffix = &caps[4];
      if variant == payload {
        format!("{prefix}{KOTLIN_PACKAGE}.{payload}{suffix}")
      } else {
        format!("{prefix}{payload}{suffix}")
      }
    })
    .into_owned();

  out = apply_kotlin_defaults(&out, wire_defaults)?;

  Ok(out)
}

/// Give every `#[serde(default)]` property its real Kotlin default value.
/// kotlinx-serialization only falls back for properties that have one, and
/// typeshare's `T? = null` loses the value serde guarantees, so the property
/// goes back to non-nullable with the resolved default.
fn apply_kotlin_defaults(input: &str, wire_defaults: &defaults::DefaultsIndex) -> Result<String> {
  let class_header = regex::Regex::new(r"^data class (\w+) \(").expect("kotlin class header regex");
  let field_line = regex::Regex::new(r"^\tval (\w+): (.+)$").expect("kotlin field regex");

  let mut out = String::with_capacity(input.len());
  let mut applied: BTreeSet<(String, String)> = BTreeSet::new();
  let mut current: Option<String> = None;

  for line in input.split_inclusive('\n') {
    let body = line.trim_end_matches('\n');
    if let Some(cap) = class_header.captures(body) {
      current = Some(cap[1].to_string());
    } else if body == ")" {
      current = None;
    }

    let patched = current.as_ref().and_then(|ty| {
      let cap = field_line.captures(body)?;
      let field = cap[1].to_string();
      let value = wire_defaults
        .get(ty)?
        .iter()
        .find(|candidate| candidate.field == field)?;
      let rest = &cap[2];
      let (declared, comma) = match rest.strip_suffix(',') {
        Some(stripped) => (stripped, ","),
        None => (rest.as_ref(), ""),
      };
      let kotlin_ty = declared
        .split_once(" = ")
        .map_or(declared, |(ty, _)| ty)
        .trim_end_matches('?');
      applied.insert((ty.clone(), field.clone()));
      Some(format!(
        "\tval {field}: {kotlin_ty} = {}{comma}\n",
        defaults::kotlin_expr(&value.value)
      ))
    });

    out.push_str(patched.as_deref().unwrap_or(line));
  }

  verify_defaults_applied(input, wire_defaults, &applied, |ty| format!("data class {ty} ("))?;
  Ok(out)
}

/// Drop adjacent identical top-level declarations from typeshare's
/// swift output. Identifies `public (struct|class|enum) NAME` blocks
/// (including preceding `///` doc lines), brace-balances their bodies,
/// and dedupes on `(kind, name, body)`. Bails if two definitions share
/// a name but differ in body.
fn dedup_swift_decls(input: &str) -> Result<String> {
  let header = regex::Regex::new(r"(?m)^public (struct|class|enum) (\w+)\b").expect("swift header regex");
  let lines: Vec<&str> = input.split_inclusive('\n').collect();
  let line_starts = line_start_offsets(input);
  let mut blocks: Vec<DeclBlock> = Vec::new();
  let mut cursor = 0usize;
  while let Some(m) = header.find_at(input, cursor) {
    let header_line_idx = line_index_for(&line_starts, m.start());
    let cap = header.captures_at(input, m.start()).expect("re-capture");
    let kind = cap.get(1).unwrap().as_str().to_string();
    let name = cap.get(2).unwrap().as_str().to_string();
    let preamble_start = preamble_start_for(&lines, header_line_idx);
    let body_end = brace_balanced_end(&lines, header_line_idx)?;
    blocks.push(DeclBlock {
      kind,
      name,
      preamble_start,
      header_line: header_line_idx,
      body_end,
    });
    let next_line = (body_end + 1).min(line_starts.len() - 1);
    cursor = line_starts[next_line].max(m.end());
  }
  finalize_dedup(&lines, blocks)
}

/// Drop adjacent identical top-level declarations from typeshare's
/// kotlin output. Identifies `data class NAME (...)` blocks (paren-
/// balanced) and `sealed class NAME { ... }` blocks (brace-balanced),
/// including preceding `@Serializable` / `///` lines. Same dedup rule
/// as the swift pass.
fn dedup_kotlin_decls(input: &str) -> Result<String> {
  let header =
    regex::Regex::new(r"(?m)^(data class|sealed class|enum class|class) (\w+)\b").expect("kotlin header regex");
  let lines: Vec<&str> = input.split_inclusive('\n').collect();
  let line_starts = line_start_offsets(input);
  let mut blocks: Vec<DeclBlock> = Vec::new();
  let mut cursor = 0usize;
  while let Some(m) = header.find_at(input, cursor) {
    let header_line_idx = line_index_for(&line_starts, m.start());
    let cap = header.captures_at(input, m.start()).expect("re-capture");
    let kind = cap.get(1).unwrap().as_str().to_string();
    let name = cap.get(2).unwrap().as_str().to_string();
    let preamble_start = preamble_start_for(&lines, header_line_idx);
    let body_end = kotlin_decl_end(&lines, header_line_idx)?;
    blocks.push(DeclBlock {
      kind,
      name,
      preamble_start,
      header_line: header_line_idx,
      body_end,
    });
    let next_line = (body_end + 1).min(line_starts.len() - 1);
    cursor = line_starts[next_line].max(m.end());
  }
  finalize_dedup(&lines, blocks)
}

struct DeclBlock {
  kind: String,
  name: String,
  preamble_start: usize,
  header_line: usize,
  body_end: usize,
}

fn line_start_offsets(input: &str) -> Vec<usize> {
  let mut out = Vec::with_capacity(input.len() / 32);
  out.push(0);
  for (i, b) in input.bytes().enumerate() {
    if b == b'\n' {
      out.push(i + 1);
    }
  }
  out.push(input.len());
  out
}

fn line_index_for(starts: &[usize], offset: usize) -> usize {
  match starts.binary_search(&offset) {
    Ok(i) => i,
    Err(i) => i.saturating_sub(1),
  }
}

/// Walk backwards from `header_line` over contiguous `///` doc lines
/// and `@Attribute` annotation lines. Stop at a blank line or any other
/// content. Returns the first line index that belongs to the block's
/// preamble (may equal `header_line` when there is no preamble).
fn preamble_start_for(lines: &[&str], header_line: usize) -> usize {
  let mut i = header_line;
  while i > 0 {
    let prev = lines[i - 1].trim_end_matches('\n');
    let trimmed = prev.trim_start();
    if trimmed.starts_with("///") || trimmed.starts_with('@') {
      i -= 1;
      continue;
    }
    break;
  }
  i
}

fn brace_balanced_end(lines: &[&str], header_line: usize) -> Result<usize> {
  let mut depth = 0i32;
  let mut started = false;
  for (idx, line) in lines.iter().enumerate().skip(header_line) {
    for c in line.chars() {
      if c == '{' {
        depth += 1;
        started = true;
      } else if c == '}' {
        depth -= 1;
        if started && depth == 0 {
          return Ok(idx);
        }
      }
    }
  }
  Err(anyhow!(
    "swift dedup: ran off end while balancing braces from line {header_line}"
  ))
}

fn kotlin_decl_end(lines: &[&str], header_line: usize) -> Result<usize> {
  let header = lines[header_line];
  let opener = if header.contains('(') {
    '('
  } else if header.contains('{') {
    '{'
  } else {
    return Err(anyhow!(
      "kotlin dedup: line {header_line} has no body opener `(` or `{{`"
    ));
  };
  let closer = if opener == '(' { ')' } else { '}' };
  let mut depth = 0i32;
  let mut started = false;
  for (idx, line) in lines.iter().enumerate().skip(header_line) {
    for c in line.chars() {
      if c == opener {
        depth += 1;
        started = true;
      } else if c == closer {
        depth -= 1;
        if started && depth == 0 {
          return Ok(idx);
        }
      }
    }
  }
  Err(anyhow!(
    "kotlin dedup: ran off end while balancing {opener}{closer} from line {header_line}"
  ))
}

fn finalize_dedup(lines: &[&str], blocks: Vec<DeclBlock>) -> Result<String> {
  let mut seen: BTreeMap<(String, String), String> = BTreeMap::new();
  let mut drop_ranges: Vec<(usize, usize)> = Vec::new();
  for b in &blocks {
    let body = canonical_body(lines, b.header_line, b.body_end);
    let key = (b.kind.clone(), b.name.clone());
    if let Some(first_body) = seen.get(&key) {
      if first_body != &body {
        return Err(anyhow!(
          "codegen dedup: two definitions named `{} {}` differ in body; this is a real type mismatch worth resolving on the rust side",
          b.kind,
          b.name
        ));
      }
      drop_ranges.push((b.preamble_start, b.body_end));
    } else {
      seen.insert(key, body);
    }
  }
  if drop_ranges.is_empty() {
    return Ok(lines.join(""));
  }
  drop_ranges.sort_by_key(|r| r.0);
  let mut out = String::with_capacity(lines.iter().map(|l| l.len()).sum());
  let mut cursor = 0usize;
  for (start, end) in drop_ranges {
    while cursor < start {
      out.push_str(lines[cursor]);
      cursor += 1;
    }
    cursor = end + 1;
    while cursor < lines.len() && lines[cursor].trim().is_empty() {
      cursor += 1;
    }
  }
  while cursor < lines.len() {
    out.push_str(lines[cursor]);
    cursor += 1;
  }
  Ok(out)
}

/// Hash-equivalent canonical body for dedup. Drops the leading doc
/// comments (which legitimately differ between two surfaces) and
/// trims line-by-line so insignificant whitespace doesn't cause
/// false-positive divergence.
fn canonical_body(lines: &[&str], header_line: usize, body_end: usize) -> String {
  let mut out = String::new();
  for line in lines.iter().take(body_end + 1).skip(header_line) {
    out.push_str(line.trim());
    out.push('\n');
  }
  out
}

#[derive(Debug, Clone)]
struct AdjacentTaggedEnum {
  name: String,
  tag: String,
}

/// Walk the lib source tree and collect every `#[typeshare]`'d enum
/// that's also tagged `#[serde(tag = "...", content = "...")]`. Those
/// are the enums typeshare emits as kotlin sealed classes that need an
/// `AdjacentTaggedSerializer` proxy. The tag string drives the
/// per-enum discriminator name in the emitted serializer.
fn discover_adjacent_tagged_enums(dir: &str) -> Result<Vec<AdjacentTaggedEnum>> {
  let mut found = Vec::new();
  for entry in walkdir::WalkDir::new(dir) {
    let entry = entry.context("walk crates/lib/src")?;
    let path = entry.path();
    if !entry.file_type().is_file() {
      continue;
    }
    if path.extension().and_then(|s| s.to_str()) != Some("rs") {
      continue;
    }
    let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let parsed = match syn::parse_file(&src) {
      Ok(p) => p,
      Err(e) => {
        eprintln!("    warning: failed to parse {}: {e}", path.display());
        continue;
      }
    };
    collect_enums_from_items(&parsed.items, &mut found);
  }
  found.sort_by(|a, b| a.name.cmp(&b.name));
  found.dedup_by(|a, b| a.name == b.name);
  Ok(found)
}

fn collect_enums_from_items(items: &[Item], out: &mut Vec<AdjacentTaggedEnum>) {
  for item in items {
    match item {
      Item::Enum(en) => {
        if let Some(tag) = typeshared_adjacent_tag(&en.attrs) {
          out.push(AdjacentTaggedEnum {
            name: en.ident.to_string(),
            tag,
          });
        }
      }
      Item::Mod(m) => {
        if let Some((_, items)) = &m.content {
          collect_enums_from_items(items, out);
        }
      }
      _ => {}
    }
  }
}

fn typeshared_adjacent_tag(attrs: &[syn::Attribute]) -> Option<String> {
  let mut typeshared = false;
  let mut tag: Option<String> = None;
  let mut has_content = false;
  for attr in attrs {
    if attr.path().is_ident("typeshare") {
      typeshared = true;
      continue;
    }
    if attr.path().is_ident("serde") {
      let Ok(nested) = attr.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
      else {
        continue;
      };
      for meta in nested {
        if let Meta::NameValue(nv) = meta {
          if nv.path.is_ident("tag")
            && let syn::Expr::Lit(syn::ExprLit {
              lit: syn::Lit::Str(s), ..
            }) = &nv.value
          {
            tag = Some(s.value());
          }
          if nv.path.is_ident("content") {
            has_content = true;
          }
        }
      }
    }
  }
  if typeshared && has_content { tag } else { None }
}

fn run(program: &str, args: &[&str]) -> Result<()> {
  let status = Command::new(program)
    .args(args)
    .status()
    .with_context(|| format!("spawn {program}"))?;
  if !status.success() {
    bail!("{program} exited with {status}");
  }
  Ok(())
}
