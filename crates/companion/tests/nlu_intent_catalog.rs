use std::{collections::BTreeSet, path::PathBuf};

use bridgething_companion::voice::intent_catalog;

const OPTIONAL: &str = "BRIDGETHING_NLU_GRAMMAR_OPTIONAL";

fn grammar_path() -> Option<PathBuf> {
  if let Some(explicit) = std::env::var_os("BRIDGETHING_NLU_GRAMMAR") {
    return Some(PathBuf::from(explicit));
  }
  let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../nlu/configs/grammar.strict.json");
  sibling.exists().then_some(sibling)
}

fn bundle_manifest_path() -> Option<PathBuf> {
  if let Some(explicit) = std::env::var_os("BRIDGETHING_NLU_BUNDLE") {
    return Some(PathBuf::from(explicit).join("manifest.json"));
  }
  let sibling =
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../nlu/results/encoder/ettin-68m-s2-bundle/manifest.json");
  sibling.exists().then_some(sibling)
}

fn grammar_intents(path: &PathBuf) -> BTreeSet<String> {
  let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
  let root: serde_json::Value = serde_json::from_str(&text).expect("grammar is json");
  root["oneOf"]
    .as_array()
    .expect("grammar has a oneOf branch list")
    .iter()
    .filter_map(|branch| branch["properties"]["intent"]["const"].as_str())
    .map(str::to_owned)
    .collect()
}

#[test]
fn catalog_matches_the_decoding_grammar() {
  let Some(path) = grammar_path() else {
    assert!(
      std::env::var_os(OPTIONAL).is_some(),
      "grammar.strict.json not found next to this checkout; point BRIDGETHING_NLU_GRAMMAR at it or set {OPTIONAL}=1"
    );
    return;
  };

  let grammar = grammar_intents(&path);
  assert!(!grammar.is_empty(), "could not parse intents out of {}", path.display());

  let catalog: BTreeSet<String> = intent_catalog::SURFACE_NAMES.iter().map(|n| (*n).to_owned()).collect();
  let unlisted: Vec<&String> = grammar.difference(&catalog).collect();
  let unadmitted: Vec<&String> = catalog.difference(&grammar).collect();
  assert!(
    unlisted.is_empty(),
    "grammar admits intents the catalog never lists: {unlisted:?}"
  );
  assert!(
    unadmitted.is_empty(),
    "catalog lists intents the grammar rejects: {unadmitted:?}"
  );
}

#[test]
fn catalog_matches_the_exported_bundle_manifest() {
  let Some(path) = bundle_manifest_path() else {
    assert!(
      std::env::var_os(OPTIONAL).is_some(),
      "exported bundle manifest not found next to this checkout; point BRIDGETHING_NLU_BUNDLE at the bundle \
       directory or set {OPTIONAL}=1"
    );
    return;
  };

  let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
  let manifest: serde_json::Value = serde_json::from_str(&text).expect("manifest is json");
  let intents: Vec<&str> = manifest["intents"]
    .as_array()
    .expect("manifest has an intent list")
    .iter()
    .map(|intent| intent["name"].as_str().expect("every manifest intent is named"))
    .collect();

  assert_eq!(intents, intent_catalog::SURFACE_NAMES.to_vec());
}

#[test]
fn label_indices_are_unique_and_stable() {
  let names = intent_catalog::SURFACE_NAMES;
  let unique: BTreeSet<&str> = names.iter().copied().collect();
  assert_eq!(
    unique.len(),
    names.len(),
    "duplicate intent names would collide label indices"
  );

  let mut sorted = names.to_vec();
  sorted.sort_unstable();
  assert_eq!(
    sorted,
    names.to_vec(),
    "label order must stay alphabetical to match the exported head"
  );

  assert_eq!(intent_catalog::name_at(0), names.first().copied());
  assert_eq!(intent_catalog::name_at(names.len() - 1), names.last().copied());
  assert_eq!(intent_catalog::name_at(names.len()), None);
}

#[test]
fn rejection_wire_values_are_not_model_classes() {
  assert!(!intent_catalog::contains(intent_catalog::NO_INTENT));
  assert!(!intent_catalog::contains(intent_catalog::CLARIFY));
}

#[test]
fn every_catalog_name_round_trips_through_lookup() {
  for (index, name) in intent_catalog::SURFACE_NAMES.iter().enumerate() {
    assert_eq!(intent_catalog::name_at(index), Some(*name));
    assert!(intent_catalog::contains(name));
  }
  assert!(!intent_catalog::contains("play"));
  assert!(!intent_catalog::contains(""));
}
