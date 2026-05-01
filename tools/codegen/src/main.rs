use std::{
  path::{Path, PathBuf},
  process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use syn::{Item, Meta};

const TS_BINDINGS_DIR: &str = "crates/lib/ts/bindings";
const SWIFT_OUTPUT: &str = "crates/lib/swift/Sources/BridgethingSchema/Generated.swift";
const KOTLIN_OUTPUT: &str = "crates/lib/kotlin/schema/src/main/kotlin/dev/bridgething/schema/Generated.kt";
const KOTLIN_PACKAGE: &str = "dev.bridgething.schema";
const LIB_SRC: &str = "crates/lib/src";

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
    "all" => {
      gen_typescript()?;
      gen_swift()?;
      gen_kotlin()?;
    }
    other => bail!("unknown target {other:?}; expected one of: ts, swift, kotlin, all"),
  }
  Ok(())
}

fn gen_typescript() -> Result<()> {
  println!("==> typescript");
  if Path::new(TS_BINDINGS_DIR).exists() {
    std::fs::remove_dir_all(TS_BINDINGS_DIR).context("clear ts/bindings")?;
  }
  run("cargo", &["test", "-p", "libbridgething", "--quiet"])?;
  run("bunx", &["prettier", TS_BINDINGS_DIR, "--write", "--log-level", "warn"])?;
  Ok(())
}

fn gen_swift() -> Result<()> {
  println!("==> swift");
  run("typeshare", &["--lang=swift", "--output-file", SWIFT_OUTPUT, LIB_SRC])?;

  let content = std::fs::read_to_string(SWIFT_OUTPUT).context("read swift output")?;
  let patched = patch_swift(&content);
  std::fs::write(SWIFT_OUTPUT, patched).context("write swift output")?;
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
    adjacent_tagged.join(", ")
  );

  let content = std::fs::read_to_string(KOTLIN_OUTPUT).context("read kotlin output")?;
  let patched = patch_kotlin(&content, &adjacent_tagged)?;
  std::fs::write(KOTLIN_OUTPUT, patched).context("write kotlin output")?;
  Ok(())
}

fn patch_swift(input: &str) -> String {
  // typeshare emits `[UInt8]` for `Vec<u8>`. Swift's Codable plus every
  // msgpack lib distinguishes Data (encodes as msgpack bin) from
  // [UInt8] (encodes as an array of int). Our wire is bin.
  let mut out = input.replace("[UInt8]", "Data");
  // Generated structs travel through actor-isolated stream events on
  // the gateway side; Swift 6 strict concurrency requires Sendable.
  // Every typeshare-emitted type is a value type whose stored fields
  // are already Sendable, so adding the conformance blanket is safe.
  out = out.replace(": Codable {", ": Codable, Sendable {");
  out = out.replace(": String, Codable {", ": String, Codable, Sendable {");
  out
}

fn patch_kotlin(input: &str, adjacent_tagged: &[String]) -> Result<String> {
  // typeshare emits `List<UByte>` for `Vec<u8>`. kotlinx-msgpack
  // encodes ByteArray as msgpack bin, but List<UByte> as an array of
  // ints. Our wire is bin.
  let mut out = input.replace("List<UByte>", "ByteArray");

  // For every adjacent-tagged enum (`#[serde(tag, content)]`), typeshare
  // emits a sealed class. kotlinx's default polymorphism for binary
  // formats sibling-inlines the discriminator into the parent map; the
  // bridgething wire shape is a nested `{<disc>: "tag", "data": payload}`
  // object. The `XSerializer` proxies in Serializers.kt produce that
  // shape, but only when the sealed class is annotated to use them.
  for name in adjacent_tagged {
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

  Ok(out)
}

/// Walk the lib source tree and collect the names of every
/// `#[typeshare]`'d enum that is also tagged
/// `#[serde(tag = "...", content = "...")]`. Those are the enums that
/// typeshare emits as kotlin sealed classes needing the
/// AdjacentTaggedSerializer proxy.
fn discover_adjacent_tagged_enums(dir: &str) -> Result<Vec<String>> {
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
  found.sort();
  found.dedup();
  Ok(found)
}

fn collect_enums_from_items(items: &[Item], out: &mut Vec<String>) {
  for item in items {
    match item {
      Item::Enum(en) if is_typeshared_adjacent_tagged(&en.attrs) => {
        out.push(en.ident.to_string());
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

fn is_typeshared_adjacent_tagged(attrs: &[syn::Attribute]) -> bool {
  let mut typeshared = false;
  let mut tagged = false;
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
      let mut has_tag = false;
      let mut has_content = false;
      for meta in nested {
        if let Meta::NameValue(nv) = meta {
          if nv.path.is_ident("tag") {
            has_tag = true;
          }
          if nv.path.is_ident("content") {
            has_content = true;
          }
        }
      }
      if has_tag && has_content {
        tagged = true;
      }
    }
  }
  typeshared && tagged
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
