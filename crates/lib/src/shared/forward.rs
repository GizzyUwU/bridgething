use bridgething_macros::BridgeEvent;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typeshare::typeshare;

#[typeshare]
#[serde_with::serde_as]
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, BridgeEvent)]
#[serde(
  tag = "encoding",
  content = "data",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "shared.ts")]
pub enum ForwardMessage {
  Text(String),
  Json(#[ts(type = "unknown")] serde_json::Value),
  Binary(
    #[serde_as(as = "serde_with::Bytes")]
    #[ts(type = "Uint8Array")]
    Vec<u8>,
  ),
}

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
// pub struct ForwardedJson(pub serde_json::Value);

// this is a gross abuse of the TS trait and it doesn't work lol
// impl TS for ForwardedJson {
//   type WithoutGenerics = Self;

//   fn decl() -> String {
//     "".to_string()
//   }

//   fn decl_concrete() -> String {
//     Self::decl()
//   }

//   fn name() -> String {
//     "any".to_string()
//   }

//   fn inline() -> String {
//     "T".to_string()
//   }

//   fn inline_flattened() -> String {
//     "T".to_string()
//   }

//   fn output_path() -> Option<&'static std::path::Path> {
//     Some(std::path::Path::new("shared.ts"))
//   }

//   fn export() -> Result<(), ts_rs::ExportError>
//   where
//     Self: 'static,
//   {
//     Ok(())
//   }
// }
