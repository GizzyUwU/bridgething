use libbridgething::wire::MsgMeta;
use uuid::Uuid;

pub trait Protocol: Send + Sync + 'static {
  type OutData: Send + 'static;
  type InData: Send + 'static;
  type OutMsg: Send + 'static;
  type InMsg: Clone + Send + 'static;

  fn envelope(id: Uuid, meta: MsgMeta, data: Self::OutData) -> Self::OutMsg;
  fn in_id(msg: &Self::InMsg) -> Uuid;
  fn in_meta(msg: &Self::InMsg) -> &MsgMeta;
  fn in_data(msg: Self::InMsg) -> Self::InData;
}
