//! aggressive interlaced inquiry scan so the device shows up fast in ios
//! settings / EAAccessoryManager while discoverable. bluez only toggles
//! inquiry scan on/off via the Discoverable property and leaves the scan
//! activity at the controller default (~11 ms window every 2.56 s, ~0.4%
//! duty), which is why discovery otherwise takes tens of seconds. neither
//! bluez D-Bus nor the mgmt api exposes scan activity, so it goes over a
//! raw HCI socket. the controller resets these to default on every adapter
//! power-on, so they are reapplied each time discoverable is enabled.

use std::{
  io,
  os::fd::{AsRawFd, OwnedFd},
};

use bluer::Adapter;

use super::hci::{hci_index, open_raw_hci};

const HCI_COMMAND_PKT: u8 = 0x01;

// Write_Inquiry_Scan_Activity (OGF 0x03, OCF 0x1E) and Write_Inquiry_Scan_Type
// (OCF 0x43). interval/window are LE counts of 0.625 ms slots: 0x0200 = 320 ms
// interval, 0x0100 = 160 ms window (50% duty), type 0x01 = interlaced.
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
