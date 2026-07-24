use std::{
  io,
  os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

use bluer::{Adapter, Address};

pub(crate) const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_HCI: libc::c_int = 1;
const HCI_CHANNEL_RAW: u16 = 0;

const HCIGETCONNLIST: libc::c_ulong = 0x800448d4;
const LE_LINK: u8 = 0x80;
const CONN_LIST_MAX: usize = 16;
const CONN_INFO_SIZE: usize = 16;
const CONN_INFO_BDADDR_OFFSET: usize = 2;
const CONN_INFO_TYPE_OFFSET: usize = 8;

pub(crate) fn hci_index(adapter: &Adapter) -> io::Result<u16> {
  adapter
    .name()
    .strip_prefix("hci")
    .and_then(|n| n.parse::<u16>().ok())
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "adapter name is not hciN"))
}

pub(crate) fn open_raw_hci(dev: u16) -> io::Result<OwnedFd> {
  let mut addr = [0u8; 6];
  addr[0..2].copy_from_slice(&(AF_BLUETOOTH as u16).to_ne_bytes());
  addr[2..4].copy_from_slice(&dev.to_ne_bytes());
  addr[4..6].copy_from_slice(&HCI_CHANNEL_RAW.to_ne_bytes());

  // SAFETY: wrapped in OwnedFd so it closes on drop, and bind only reads addr for its len
  unsafe {
    let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_RAW | libc::SOCK_CLOEXEC, BTPROTO_HCI);
    if fd < 0 {
      return Err(io::Error::last_os_error());
    }
    let sock = OwnedFd::from_raw_fd(fd);
    if libc::bind(
      fd,
      addr.as_ptr() as *const libc::sockaddr,
      addr.len() as libc::socklen_t,
    ) < 0
    {
      return Err(io::Error::last_os_error());
    }
    Ok(sock)
  }
}

pub(crate) fn le_acl_connected(adapter: &Adapter, address: Address) -> io::Result<bool> {
  let dev = hci_index(adapter)?;
  let sock = open_raw_hci(dev)?;

  let mut buf = [0u8; 4 + CONN_LIST_MAX * CONN_INFO_SIZE];
  buf[0..2].copy_from_slice(&dev.to_ne_bytes());
  buf[2..4].copy_from_slice(&(CONN_LIST_MAX as u16).to_ne_bytes());

  // SAFETY: HCIGETCONNLIST reads dev_id/conn_num and fills conn_info entries within the buffer we own
  let rc = unsafe { libc::ioctl(sock.as_raw_fd(), HCIGETCONNLIST, buf.as_mut_ptr()) };
  if rc < 0 {
    return Err(io::Error::last_os_error());
  }

  let filled = u16::from_ne_bytes([buf[2], buf[3]]) as usize;
  let mut wire_addr = address.0;
  wire_addr.reverse();
  for i in 0..filled.min(CONN_LIST_MAX) {
    let entry = &buf[4 + i * CONN_INFO_SIZE..4 + (i + 1) * CONN_INFO_SIZE];
    if entry[CONN_INFO_TYPE_OFFSET] == LE_LINK
      && entry[CONN_INFO_BDADDR_OFFSET..CONN_INFO_BDADDR_OFFSET + 6] == wire_addr
    {
      return Ok(true);
    }
  }
  Ok(false)
}
