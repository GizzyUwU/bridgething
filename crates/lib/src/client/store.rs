use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::client::{ClientCommandType, ClientRequest};
use crate::impl_client_request;
use crate::server::{ServerEventData, ServerStorageEvent, StorageResponse};

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(
  tag = "action",
  content = "args",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "client.ts")]
pub enum ClientKVStoreCommand {
  Get { key: String },
  Put { key: String, value: String },
  Delete { key: String },
}

/// Webapp request: read a value out of KV storage by key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KVGet {
  pub key: String,
}

/// Webapp request: write a value to KV storage under `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KVPut {
  pub key: String,
  pub value: String,
}

/// Webapp request: delete the KV storage entry at `key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KVDelete {
  pub key: String,
}

impl_client_request! {
  request: KVGet,
  response: StorageResponse,
  encode_request:
    r => ClientCommandType::Store(ClientKVStoreCommand::Get { key: r.key }),
  extract_response:
    ServerEventData::Storage(ServerStorageEvent::Response(v)) => v,
  encode_response:
    v => ServerEventData::Storage(ServerStorageEvent::Response(v)),
}

impl_client_request! {
  request: KVPut,
  response: StorageResponse,
  encode_request:
    r => ClientCommandType::Store(ClientKVStoreCommand::Put { key: r.key, value: r.value }),
  extract_response:
    ServerEventData::Storage(ServerStorageEvent::Response(v)) => v,
  encode_response:
    v => ServerEventData::Storage(ServerStorageEvent::Response(v)),
}

impl_client_request! {
  request: KVDelete,
  response: StorageResponse,
  encode_request:
    r => ClientCommandType::Store(ClientKVStoreCommand::Delete { key: r.key }),
  extract_response:
    ServerEventData::Storage(ServerStorageEvent::Response(v)) => v,
  encode_response:
    v => ServerEventData::Storage(ServerStorageEvent::Response(v)),
}
