//! Resolve `#[serde(default)]` on wire structs into per-language default
//! expressions.
//!
//! A field without `#[serde(default)]` is a required key in Swift and Kotlin,
//! so adding one to a wire struct hard-fails decode against any peer that
//! predates it. The daemon and the companion ship on independent update
//! channels, so that skew is the normal case, not an edge case.
//!
//! `#[serde(default)]` is the tolerance marker, but typeshare renders those
//! fields as plain optionals: decodable, yet stripped of the value serde
//! guarantees. Every consumer then has to re-derive the default, and one that
//! guesses wrong reads `nil` as false on a field that defaults to true.
//!
//! This walks `crates/lib/src`, computes what `Default::default()` produces
//! for each defaulted field, and hands the emitters a language-neutral IR so
//! the field can stay non-optional and still tolerate a missing key.
//! Anything it cannot prove is a hard error rather than a silent guess.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, bail};
use syn::{Attribute, Expr, Fields, Item, ItemEnum, ItemFn, ItemStruct, Lit, Meta, Type, UnOp};

/// Width tag carried alongside numeric defaults. Swift infers a literal's
/// type from context; Kotlin needs `0u` for the unsigned types and `0.0f`
/// for `Float`, so the Rust primitive has to survive into rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumKind {
  Signed,
  Unsigned,
  F32,
  F64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DefaultValue {
  Bool(bool),
  Num { text: String, kind: NumKind },
  Str(String),
  Null,
  EmptyList,
  EmptyMap,
  EnumVariant { ty: String, variant: String },
  Struct { ty: String, fields: Vec<(String, DefaultValue)> },
}

#[derive(Debug, Clone)]
pub struct FieldDefault {
  /// Emitted property identifier, which is the lower-camel Rust ident.
  pub field: String,
  pub value: DefaultValue,
}

/// Defaulted fields per `#[typeshare]`'d struct name.
#[derive(Debug, Default)]
pub struct DefaultsIndex {
  pub by_type: BTreeMap<String, Vec<FieldDefault>>,
}

impl DefaultsIndex {
  pub fn get(&self, ty: &str) -> Option<&[FieldDefault]> {
    self.by_type.get(ty).map(Vec::as_slice)
  }

  pub fn field_count(&self) -> usize {
    self.by_type.values().map(Vec::len).sum()
  }
}

/// How a field asked to be defaulted.
#[derive(Debug, Clone)]
enum DefaultSpec {
  /// `#[serde(default)]`, meaning `Default::default()` for the field type.
  Derived,
  /// `#[serde(default = "path")]`, meaning the named function's return value.
  Path(String),
}

#[derive(Default)]
struct SourceIndex {
  structs: BTreeMap<String, ItemStruct>,
  enums: BTreeMap<String, ItemEnum>,
  fns: BTreeMap<String, ItemFn>,
  /// `impl Default for X` bodies, keyed by X.
  default_impls: BTreeMap<String, Expr>,
}

pub fn discover(dir: &str) -> Result<DefaultsIndex> {
  let index = SourceIndex::build(dir)?;
  let mut out = DefaultsIndex::default();

  for (name, item) in &index.structs {
    if !has_typeshare(&item.attrs) {
      continue;
    }
    let container_default = container_serde_default(&item.attrs);
    let mut fields = Vec::new();
    for field in named_fields(item) {
      let Some(ident) = field.ident.as_ref() else {
        continue;
      };
      let spec = match (field_serde_default(&field.attrs)?, container_default) {
        (Some(spec), _) => spec,
        (None, true) => DefaultSpec::Derived,
        (None, false) => continue,
      };
      // Optionals already decode tolerantly: Swift synthesizes
      // `decodeIfPresent` for them and typeshare emits `= null` in Kotlin.
      if is_option(&field.ty) {
        continue;
      }
      let value = index
        .resolve(&spec, &field.ty, None, &mut Vec::new())
        .with_context(|| format!("resolving default for {name}.{ident}"))?;
      fields.push(FieldDefault {
        field: lower_camel(&ident.to_string()),
        value,
      });
    }
    if !fields.is_empty() {
      out.by_type.insert(name.clone(), fields);
    }
  }

  Ok(out)
}

impl SourceIndex {
  fn build(dir: &str) -> Result<Self> {
    let mut index = Self::default();
    for entry in walkdir::WalkDir::new(dir) {
      let entry = entry.with_context(|| format!("walk {dir}"))?;
      let path = entry.path();
      if !entry.file_type().is_file() || path.extension().and_then(|s| s.to_str()) != Some("rs") {
        continue;
      }
      let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
      let parsed = match syn::parse_file(&src) {
        Ok(parsed) => parsed,
        Err(err) => {
          eprintln!("    warning: failed to parse {}: {err}", path.display());
          continue;
        }
      };
      index.collect(&parsed.items);
    }
    Ok(index)
  }

  fn collect(&mut self, items: &[Item]) {
    for item in items {
      match item {
        Item::Struct(s) => {
          self.structs.insert(s.ident.to_string(), s.clone());
        }
        Item::Enum(e) => {
          self.enums.insert(e.ident.to_string(), e.clone());
        }
        Item::Fn(f) => {
          self.fns.insert(f.sig.ident.to_string(), f.clone());
        }
        Item::Impl(imp) => self.collect_default_impl(imp),
        Item::Mod(m) => {
          if let Some((_, items)) = &m.content {
            self.collect(items);
          }
        }
        _ => {}
      }
    }
  }

  fn collect_default_impl(&mut self, imp: &syn::ItemImpl) {
    let Some((_, path, _)) = &imp.trait_ else { return };
    if path.segments.last().map(|s| s.ident.to_string()).as_deref() != Some("Default") {
      return;
    }
    let Type::Path(self_ty) = imp.self_ty.as_ref() else {
      return;
    };
    let Some(name) = self_ty.path.segments.last().map(|s| s.ident.to_string()) else {
      return;
    };
    for item in &imp.items {
      if let syn::ImplItem::Fn(f) = item
        && f.sig.ident == "default"
        && let Ok(expr) = tail_expr(&f.block)
      {
        self.default_impls.insert(name, expr);
        return;
      }
    }
  }

  fn resolve(&self, spec: &DefaultSpec, ty: &Type, self_ty: Option<&str>, stack: &mut Vec<String>) -> Result<DefaultValue> {
    match spec {
      DefaultSpec::Derived => self.default_for_type(ty, stack),
      DefaultSpec::Path(path) => {
        let func = self.fns.get(path).ok_or_else(|| {
          anyhow!("#[serde(default = \"{path}\")] names a function codegen cannot find under crates/lib/src")
        })?;
        let expr = tail_expr(&func.block)
          .with_context(|| format!("#[serde(default = \"{path}\")] must end in a single expression"))?;
        self.eval(&expr, self_ty, stack)
      }
    }
  }

  fn default_for_type(&self, ty: &Type, stack: &mut Vec<String>) -> Result<DefaultValue> {
    let Type::Path(path) = ty else {
      bail!("codegen cannot derive a default for the non-path type `{}`", type_text(ty));
    };
    let Some(segment) = path.path.segments.last() else {
      bail!("empty type path");
    };
    let name = segment.ident.to_string();
    Ok(match name.as_str() {
      "bool" => DefaultValue::Bool(false),
      "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => DefaultValue::Num {
        text: "0".into(),
        kind: NumKind::Unsigned,
      },
      "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => DefaultValue::Num {
        text: "0".into(),
        kind: NumKind::Signed,
      },
      "f32" => DefaultValue::Num {
        text: "0.0".into(),
        kind: NumKind::F32,
      },
      "f64" => DefaultValue::Num {
        text: "0.0".into(),
        kind: NumKind::F64,
      },
      "String" => DefaultValue::Str(String::new()),
      "Option" => DefaultValue::Null,
      "Vec" | "VecDeque" | "HashSet" | "BTreeSet" => DefaultValue::EmptyList,
      "HashMap" | "BTreeMap" => DefaultValue::EmptyMap,
      _ => self.default_for_named(&name, stack)?,
    })
  }

  fn default_for_named(&self, name: &str, stack: &mut Vec<String>) -> Result<DefaultValue> {
    if stack.iter().any(|seen| seen == name) {
      bail!("`{name}` defaults through itself; codegen cannot materialize a cyclic default");
    }
    stack.push(name.to_string());
    let result = self.default_for_named_inner(name, stack);
    stack.pop();
    result
  }

  fn default_for_named_inner(&self, name: &str, stack: &mut Vec<String>) -> Result<DefaultValue> {
    if let Some(expr) = self.default_impls.get(name) {
      return self.eval(&expr.clone(), Some(name), stack);
    }
    if let Some(item) = self.enums.get(name) {
      return self.default_enum_variant(item);
    }
    if let Some(item) = self.structs.get(name) {
      if !derives_default(&item.attrs) {
        bail!(
          "`{name}` is used as a `#[serde(default)]` field type but neither derives Default nor has an `impl Default`"
        );
      }
      let container_default = container_serde_default(&item.attrs);
      let mut fields = Vec::new();
      for field in named_fields(item) {
        let Some(ident) = field.ident.as_ref() else {
          continue;
        };
        let spec = match (field_serde_default(&field.attrs)?, container_default) {
          (Some(spec), _) => spec,
          (None, true) => DefaultSpec::Derived,
          (None, false) => DefaultSpec::Derived,
        };
        let value = self
          .resolve(&spec, &field.ty, Some(name), stack)
          .with_context(|| format!("in `{name}.{ident}`"))?;
        fields.push((lower_camel(&ident.to_string()), value));
      }
      return Ok(DefaultValue::Struct {
        ty: name.to_string(),
        fields,
      });
    }
    bail!("`{name}` is used as a `#[serde(default)]` field type but is not defined under crates/lib/src")
  }

  fn default_enum_variant(&self, item: &ItemEnum) -> Result<DefaultValue> {
    if !derives_default(&item.attrs) {
      bail!(
        "enum `{}` is used as a `#[serde(default)]` field type but does not derive Default",
        item.ident
      );
    }
    let marked = item
      .variants
      .iter()
      .find(|v| v.attrs.iter().any(|a| a.path().is_ident("default")));
    let variant = marked.ok_or_else(|| {
      anyhow!(
        "enum `{}` derives Default but has no `#[default]` variant codegen can name",
        item.ident
      )
    })?;
    if !matches!(variant.fields, Fields::Unit) {
      bail!(
        "enum `{}` defaults to non-unit variant `{}`; codegen only materializes unit variants",
        item.ident,
        variant.ident
      );
    }
    Ok(DefaultValue::EnumVariant {
      ty: item.ident.to_string(),
      variant: variant.ident.to_string(),
    })
  }

  fn eval(&self, expr: &Expr, self_ty: Option<&str>, stack: &mut Vec<String>) -> Result<DefaultValue> {
    match expr {
      Expr::Lit(lit) => literal_value(&lit.lit),
      Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => match self.eval(&unary.expr, self_ty, stack)? {
        DefaultValue::Num { text, kind } => Ok(DefaultValue::Num {
          text: format!("-{text}"),
          kind,
        }),
        other => bail!("cannot negate the default value {other:?}"),
      },
      Expr::Group(group) => self.eval(&group.expr, self_ty, stack),
      Expr::Paren(paren) => self.eval(&paren.expr, self_ty, stack),
      Expr::Path(path) => self.eval_path(&path.path, self_ty),
      Expr::Call(call) => self.eval_call(call, self_ty, stack),
      Expr::Struct(structure) => self.eval_struct(structure, self_ty, stack),
      Expr::Macro(mac) if mac.mac.path.is_ident("vec") => {
        if mac.mac.tokens.is_empty() {
          Ok(DefaultValue::EmptyList)
        } else {
          bail!("codegen only materializes an empty `vec![]` default")
        }
      }
      other => bail!(
        "codegen cannot evaluate the default expression `{}`",
        quote::quote!(#other)
      ),
    }
  }

  fn eval_path(&self, path: &syn::Path, self_ty: Option<&str>) -> Result<DefaultValue> {
    let owned: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let segments: Vec<&str> = owned.iter().map(String::as_str).collect();
    match segments.as_slice() {
      ["None"] => Ok(DefaultValue::Null),
      [ty, variant] => {
        let ty = if *ty == "Self" {
          self_ty
            .ok_or_else(|| anyhow!("`Self::{variant}` used where codegen does not know the enclosing type"))?
            .to_string()
        } else {
          (*ty).to_string()
        };
        Ok(DefaultValue::EnumVariant {
          ty,
          variant: (*variant).to_string(),
        })
      }
      _ => bail!("codegen cannot evaluate the default path `{}`", quote::quote!(#path)),
    }
  }

  fn eval_call(&self, call: &syn::ExprCall, self_ty: Option<&str>, stack: &mut Vec<String>) -> Result<DefaultValue> {
    let Expr::Path(callee) = call.func.as_ref() else {
      bail!("codegen cannot evaluate a computed default call");
    };
    let owned: Vec<String> = callee.path.segments.iter().map(|s| s.ident.to_string()).collect();
    let segments: Vec<&str> = owned.iter().map(String::as_str).collect();
    match segments.as_slice() {
      ["String", "new"] => Ok(DefaultValue::Str(String::new())),
      [ty, "new"] if matches!(*ty, "Vec" | "VecDeque" | "HashSet" | "BTreeSet") => Ok(DefaultValue::EmptyList),
      [ty, "new"] if matches!(*ty, "HashMap" | "BTreeMap") => Ok(DefaultValue::EmptyMap),
      ["Some"] => {
        let inner = call
          .args
          .first()
          .ok_or_else(|| anyhow!("`Some()` default with no argument"))?;
        self.eval(inner, self_ty, stack)
      }
      ["Default", "default"] => {
        let ty = self_ty.ok_or_else(|| anyhow!("bare `Default::default()` where codegen does not know the type"))?;
        self.default_for_named(ty, stack)
      }
      [ty, "default"] => {
        let ty = if *ty == "Self" {
          self_ty
            .ok_or_else(|| anyhow!("`Self::default()` where codegen does not know the enclosing type"))?
            .to_string()
        } else {
          (*ty).to_string()
        };
        self.default_for_named(&ty, stack)
      }
      _ => bail!(
        "codegen cannot evaluate the default call `{}`",
        quote::quote!(#callee)
      ),
    }
  }

  fn eval_struct(&self, structure: &syn::ExprStruct, self_ty: Option<&str>, stack: &mut Vec<String>) -> Result<DefaultValue> {
    let named = structure
      .path
      .segments
      .last()
      .map(|s| s.ident.to_string())
      .ok_or_else(|| anyhow!("struct default with an empty path"))?;
    let ty = if named == "Self" {
      self_ty
        .ok_or_else(|| anyhow!("`Self {{ .. }}` default where codegen does not know the enclosing type"))?
        .to_string()
    } else {
      named
    };

    let mut fields: Vec<(String, DefaultValue)> = Vec::new();
    let mut written: BTreeSet<String> = BTreeSet::new();
    for field in &structure.fields {
      let syn::Member::Named(ident) = &field.member else {
        bail!("codegen only materializes struct defaults with named fields");
      };
      let value = self
        .eval(&field.expr, Some(&ty), stack)
        .with_context(|| format!("in `{ty}.{ident}`"))?;
      written.insert(ident.to_string());
      fields.push((lower_camel(&ident.to_string()), value));
    }

    // `..Default::default()` leaves the untouched fields to the type's own
    // derive, so fill them the same way the derive would.
    if structure.rest.is_some() {
      let item = self
        .structs
        .get(&ty)
        .ok_or_else(|| anyhow!("`{ty}` has a `..` default rest but is not defined under crates/lib/src"))?
        .clone();
      for field in named_fields(&item) {
        let Some(ident) = field.ident.as_ref() else { continue };
        if written.contains(&ident.to_string()) {
          continue;
        }
        let spec = field_serde_default(&field.attrs)?.unwrap_or(DefaultSpec::Derived);
        let value = self
          .resolve(&spec, &field.ty, Some(&ty), stack)
          .with_context(|| format!("in `{ty}.{ident}`"))?;
        fields.push((lower_camel(&ident.to_string()), value));
      }
      reorder_to_declaration(&item, &mut fields);
    }

    Ok(DefaultValue::Struct { ty, fields })
  }
}

/// Both emitters build a positional/named argument list against typeshare's
/// memberwise initializer, so field order has to match the Rust declaration.
fn reorder_to_declaration(item: &ItemStruct, fields: &mut Vec<(String, DefaultValue)>) {
  let order: Vec<String> = named_fields(item)
    .filter_map(|f| f.ident.as_ref().map(|i| lower_camel(&i.to_string())))
    .collect();
  fields.sort_by_key(|(name, _)| order.iter().position(|o| o == name).unwrap_or(usize::MAX));
}

fn literal_value(lit: &Lit) -> Result<DefaultValue> {
  Ok(match lit {
    Lit::Bool(b) => DefaultValue::Bool(b.value),
    Lit::Str(s) => DefaultValue::Str(s.value()),
    Lit::Int(i) => {
      let kind = if i.suffix().starts_with('u') {
        NumKind::Unsigned
      } else {
        NumKind::Signed
      };
      DefaultValue::Num {
        text: i.base10_digits().to_string(),
        kind,
      }
    }
    Lit::Float(f) => {
      let kind = if f.suffix() == "f32" { NumKind::F32 } else { NumKind::F64 };
      DefaultValue::Num {
        text: f.base10_digits().to_string(),
        kind,
      }
    }
    other => bail!("codegen cannot materialize the literal default `{}`", quote::quote!(#other)),
  })
}

// MARK: - rendering

pub fn swift_expr(value: &DefaultValue) -> String {
  match value {
    DefaultValue::Bool(b) => b.to_string(),
    DefaultValue::Num { text, .. } => text.clone(),
    DefaultValue::Str(s) => format!("\"{}\"", escape(s)),
    DefaultValue::Null => "nil".into(),
    DefaultValue::EmptyList => "[]".into(),
    DefaultValue::EmptyMap => "[:]".into(),
    DefaultValue::EnumVariant { variant, .. } => format!(".{}", lower_camel_from_pascal(variant)),
    DefaultValue::Struct { ty, fields } => {
      let args: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{name}: {}", swift_expr(value)))
        .collect();
      format!("{ty}({})", args.join(", "))
    }
  }
}

pub fn kotlin_expr(value: &DefaultValue) -> String {
  match value {
    DefaultValue::Bool(b) => b.to_string(),
    DefaultValue::Num { text, kind } => match kind {
      NumKind::Unsigned => format!("{text}u"),
      NumKind::F32 => format!("{text}f"),
      NumKind::Signed | NumKind::F64 => text.clone(),
    },
    DefaultValue::Str(s) => format!("\"{}\"", escape(s)),
    DefaultValue::Null => "null".into(),
    DefaultValue::EmptyList => "emptyList()".into(),
    DefaultValue::EmptyMap => "emptyMap()".into(),
    DefaultValue::EnumVariant { ty, variant } => format!("{ty}.{variant}"),
    DefaultValue::Struct { ty, fields } => {
      let args: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{name} = {}", kotlin_expr(value)))
        .collect();
      format!("{ty}({})", args.join(", "))
    }
  }
}

fn escape(raw: &str) -> String {
  raw.replace('\\', "\\\\").replace('"', "\\\"")
}

// MARK: - attribute helpers

fn has_typeshare(attrs: &[Attribute]) -> bool {
  attrs.iter().any(|a| a.path().is_ident("typeshare"))
}

fn derives_default(attrs: &[Attribute]) -> bool {
  attrs.iter().any(|attr| {
    if !attr.path().is_ident("derive") {
      return false;
    }
    attr
      .parse_args_with(syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
      .map(|paths| paths.iter().any(|p| p.is_ident("Default")))
      .unwrap_or(false)
  })
}

fn container_serde_default(attrs: &[Attribute]) -> bool {
  serde_metas(attrs)
    .iter()
    .any(|meta| matches!(meta, Meta::Path(p) if p.is_ident("default")))
}

fn field_serde_default(attrs: &[Attribute]) -> Result<Option<DefaultSpec>> {
  for meta in serde_metas(attrs) {
    match meta {
      Meta::Path(p) if p.is_ident("default") => return Ok(Some(DefaultSpec::Derived)),
      Meta::NameValue(nv) if nv.path.is_ident("default") => {
        let Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) = &nv.value else {
          bail!("`#[serde(default = ...)]` expects a function-path string");
        };
        return Ok(Some(DefaultSpec::Path(s.value())));
      }
      _ => {}
    }
  }
  Ok(None)
}

fn serde_metas(attrs: &[Attribute]) -> Vec<Meta> {
  let mut out = Vec::new();
  for attr in attrs {
    if !attr.path().is_ident("serde") {
      continue;
    }
    if let Ok(nested) = attr.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated) {
      out.extend(nested);
    }
  }
  out
}

fn named_fields(item: &ItemStruct) -> impl Iterator<Item = &syn::Field> {
  match &item.fields {
    Fields::Named(named) => Some(named.named.iter()),
    _ => None,
  }
  .into_iter()
  .flatten()
}

fn is_option(ty: &Type) -> bool {
  let Type::Path(path) = ty else { return false };
  path
    .path
    .segments
    .last()
    .map(|s| s.ident == "Option")
    .unwrap_or(false)
}

fn tail_expr(block: &syn::Block) -> Result<Expr> {
  match block.stmts.last() {
    Some(syn::Stmt::Expr(expr, None)) => Ok(expr.clone()),
    _ => bail!("expected the body to end in a bare expression"),
  }
}

fn type_text(ty: &Type) -> String {
  quote::quote!(#ty).to_string()
}

// MARK: - identifier casing

/// snake_case Rust field ident to the lower-camel identifier typeshare emits.
pub fn lower_camel(snake: &str) -> String {
  let mut out = String::with_capacity(snake.len());
  let mut upper_next = false;
  for ch in snake.chars() {
    if ch == '_' {
      upper_next = true;
      continue;
    }
    if upper_next {
      out.extend(ch.to_uppercase());
      upper_next = false;
    } else {
      out.push(ch);
    }
  }
  out
}

/// PascalCase Rust variant ident to the lower-camel case identifier
/// typeshare emits for a Swift enum case.
pub fn lower_camel_from_pascal(pascal: &str) -> String {
  let mut chars = pascal.chars();
  match chars.next() {
    Some(first) => first.to_lowercase().chain(chars).collect(),
    None => String::new(),
  }
}

/// lower-camel identifier to PascalCase, for building generated type names.
pub fn pascal(lower_camel: &str) -> String {
  let mut chars = lower_camel.chars();
  match chars.next() {
    Some(first) => first.to_uppercase().chain(chars).collect(),
    None => String::new(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn index_from(src: &str) -> SourceIndex {
    let mut index = SourceIndex::default();
    index.collect(&syn::parse_file(src).expect("parse").items);
    index
  }

  fn resolve_field(src: &str, ty: &str, field: &str) -> Result<DefaultValue> {
    let index = index_from(src);
    let item = index.structs.get(ty).expect("struct").clone();
    let container = container_serde_default(&item.attrs);
    let target = named_fields(&item)
      .find(|f| f.ident.as_ref().map(|i| i == field).unwrap_or(false))
      .expect("field")
      .clone();
    let spec = match (field_serde_default(&target.attrs).expect("attrs"), container) {
      (Some(spec), _) => spec,
      (None, true) => DefaultSpec::Derived,
      (None, false) => panic!("field carries no serde default"),
    };
    index.resolve(&spec, &target.ty, None, &mut Vec::new())
  }

  #[test]
  fn scalars_and_collections_take_the_zero_value() {
    let src = r#"
      #[typeshare]
      pub struct A {
        #[serde(default)] pub flag: bool,
        #[serde(default)] pub count: u32,
        #[serde(default)] pub name: String,
        #[serde(default)] pub items: Vec<String>,
      }
    "#;
    assert_eq!(resolve_field(src, "A", "flag").unwrap(), DefaultValue::Bool(false));
    assert_eq!(
      resolve_field(src, "A", "count").unwrap(),
      DefaultValue::Num {
        text: "0".into(),
        kind: NumKind::Unsigned
      }
    );
    assert_eq!(resolve_field(src, "A", "name").unwrap(), DefaultValue::Str(String::new()));
    assert_eq!(resolve_field(src, "A", "items").unwrap(), DefaultValue::EmptyList);
  }

  #[test]
  fn an_enum_resolves_to_its_marked_default_variant() {
    let src = r#"
      #[derive(Default)]
      pub enum Role { #[default] Standard, Launcher }
      #[typeshare]
      pub struct A { #[serde(default)] pub role: Role }
    "#;
    assert_eq!(
      resolve_field(src, "A", "role").unwrap(),
      DefaultValue::EnumVariant {
        ty: "Role".into(),
        variant: "Standard".into()
      }
    );
  }

  #[test]
  fn a_manual_default_impl_is_read_field_by_field() {
    let src = r#"
      pub struct Overlays { pub call: bool, pub volume: bool }
      impl Default for Overlays {
        fn default() -> Self { Self { call: true, volume: true } }
      }
      #[typeshare]
      pub struct A { #[serde(default)] pub overlays: Overlays }
    "#;
    assert_eq!(
      resolve_field(src, "A", "overlays").unwrap(),
      DefaultValue::Struct {
        ty: "Overlays".into(),
        fields: vec![
          ("call".into(), DefaultValue::Bool(true)),
          ("volume".into(), DefaultValue::Bool(true)),
        ]
      }
    );
  }

  #[test]
  fn a_named_default_fn_is_followed_to_its_literal() {
    let src = r#"
      fn on() -> bool { true }
      #[typeshare]
      pub struct A { #[serde(default = "on")] pub notifications: bool }
    "#;
    assert_eq!(
      resolve_field(src, "A", "notifications").unwrap(),
      DefaultValue::Bool(true)
    );
  }

  #[test]
  fn a_derived_struct_default_recurses_through_its_fields() {
    let src = r#"
      #[derive(Default)]
      pub struct Slots { pub artist: Option<String>, pub level: u32 }
      #[typeshare]
      pub struct A { #[serde(default)] pub slots: Slots }
    "#;
    assert_eq!(
      resolve_field(src, "A", "slots").unwrap(),
      DefaultValue::Struct {
        ty: "Slots".into(),
        fields: vec![
          ("artist".into(), DefaultValue::Null),
          (
            "level".into(),
            DefaultValue::Num {
              text: "0".into(),
              kind: NumKind::Unsigned
            }
          ),
        ]
      }
    );
  }

  #[test]
  fn a_default_codegen_cannot_prove_is_an_error_not_a_required_key() {
    let src = r#"
      pub struct Opaque { pub inner: u32 }
      #[typeshare]
      pub struct A { #[serde(default)] pub opaque: Opaque }
    "#;
    let err = resolve_field(src, "A", "opaque").expect_err("must not silently succeed");
    assert!(
      format!("{err:#}").contains("neither derives Default"),
      "unexpected error: {err:#}"
    );
  }

  #[test]
  fn a_cyclic_default_is_rejected() {
    let src = r#"
      #[derive(Default)]
      pub struct Node { pub next: Node }
      #[typeshare]
      pub struct A { #[serde(default)] pub node: Node }
    "#;
    let err = resolve_field(src, "A", "node").expect_err("must not recurse forever");
    assert!(format!("{err:#}").contains("cyclic"), "unexpected error: {err:#}");
  }

  #[test]
  fn rendering_matches_each_language_idiom() {
    let slots = DefaultValue::Struct {
      ty: "Slots".into(),
      fields: vec![
        ("artist".into(), DefaultValue::Null),
        (
          "level".into(),
          DefaultValue::Num {
            text: "0".into(),
            kind: NumKind::Unsigned,
          },
        ),
      ],
    };
    assert_eq!(swift_expr(&slots), "Slots(artist: nil, level: 0)");
    assert_eq!(kotlin_expr(&slots), "Slots(artist = null, level = 0u)");

    let role = DefaultValue::EnumVariant {
      ty: "WebappRole".into(),
      variant: "Standard".into(),
    };
    assert_eq!(swift_expr(&role), ".standard");
    assert_eq!(kotlin_expr(&role), "WebappRole.Standard");

    assert_eq!(swift_expr(&DefaultValue::EmptyList), "[]");
    assert_eq!(kotlin_expr(&DefaultValue::EmptyList), "emptyList()");
  }
}
