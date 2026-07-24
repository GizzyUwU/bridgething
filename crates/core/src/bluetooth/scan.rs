use std::{
  io,
  os::fd::{AsRawFd, OwnedFd},
};

use bluer::Adapter;

use super::hci::{hci_index, open_raw_hci};

const HCI_COMMAND_PKT: u8 = 0x01;

const OP_WRITE_INQ_SCAN_ACTIVITY: u16 = 0x0c1e;
const OP_WRITE_INQ_SCAN_TYPE: u16 = 0x0c43;
const INQ_SCAN_INTERVAL: u16 = 0x0200;
const INQ_SCAN_WINDOW: u16 = 0x0100;
const INQ_SCAN_TYPE_INTERLACED: u8 = 0x01;

pub(crate) fn apply_fast_inquiry_scan(adapter: &Adapter) -> io::Result<()> {
  let dev = hci_index(adapter)?;
  let sock = open_raw_hci(dev)?;

  let interval = INQ_SCAN_INTERVAL.to_le_bytes();
  let window = INQ_SCAN_WINDOW.to_le_bytes();
  send_command(
    &sock,
    OP_WRITE_INQ_SCAN_ACTIVITY,
    &[interval[0], interval[1], window[0], window[1]],
  )?;
  send_command(&sock, OP_WRITE_INQ_SCAN_TYPE, &[INQ_SCAN_TYPE_INTERLACED])?;
  Ok(())
}

fn send_command(sock: &OwnedFd, opcode: u16, params: &[u8]) -> io::Result<()> {
  let opcode = opcode.to_le_bytes();
  let mut packet = Vec::with_capacity(4 + params.len());
  packet.push(HCI_COMMAND_PKT);
  packet.push(opcode[0]);
  packet.push(opcode[1]);
  packet.push(params.len() as u8);
  packet.extend_from_slice(params);

  // SAFETY: writing our owned buffer to the bound raw HCI socket fd.
  let written = unsafe { libc::write(sock.as_raw_fd(), packet.as_ptr() as *const libc::c_void, packet.len()) };
  if written < 0 {
    return Err(io::Error::last_os_error());
  }
  Ok(())
}
