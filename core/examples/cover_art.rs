use bluer::l2cap::{Opts, Socket, SocketAddr, Stream};
use bluer::{Address, AddressType};
use std::fs::File;
use std::io::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// import os
// import bluetooth
// import time
// from enum import Enum

// class devType(Enum):
//     UNKNOWN = 0
//     iOS = 1
//     ANDROID = 2

// class obexDevice:
//     def __init__(self, bt_addr):
//         print("Device:", bt_addr)
//         self.dev_type = devType.UNKNOWN
//         self.bt_addr = bt_addr
//         self.sock = bluetooth.BluetoothSocket(bluetooth.L2CAP)

//     def connect(self):
//         try:
//             print("Trying Android connection")
//             psm = 0x1001  # Typical Android PSM - Get this from dbus
//             self.obex_conn_id, self.dev_type = connectObex(self, psm)

//         except: # If connection fails on 0x1001, device is probably an iPhone
//             print("Not Android. Trying iOS connection")
//             psm = 0x1007  # Typical iOS PSM - Get this from dbus
//             self.obex_conn_id, self.dev_type = connectObex(self, psm)

//     def getImage(self, handle):
//         if self.dev_type is devType.ANDROID:
//             getAndroidImage(self.sock, self.obex_conn_id, handle)
//         elif self.dev_type is devType.iOS:
//             getiOSImage(self.sock, self.obex_conn_id)

// obexConnReq = bytes.fromhex("80001a1500069b4600137163dd544a7e11e2b47c0050c2490048")
// def connectObex(device: obexDevice, psm):
//     device.sock = bluetooth.BluetoothSocket(bluetooth.L2CAP)
//     device.sock.set_l2cap_mtu(1024)
//     device.sock.set_l2cap_options([1024, 1024, 65535, 3, 1, 16, 63])
//     print(f"Connecting to {device.bt_addr} on PSM {psm}")
//     device.sock.connect((device.bt_addr, psm))
//     print(f"Connected to {device.bt_addr} on PSM {psm}. Requesting OBEX Connection...")
//     device.sock.send(obexConnReq)
//     obexResp = device.sock.recv(1024)
//     print(obexResp.hex())
//     if obexResp[0] != 160: # 0xA0/160 is a success response
//         print("OBEX Handshake failed.")
//         quit()
//     else:
//         conn_id = obexResp[8:12]
//         if conn_id == bytes.fromhex("00000001"):
//             print("Connection ID is 1. Assuming Android.")
//             dev_type = devType.ANDROID
//         else:
//             print("Connection ID is not 1. Assuming iOS.")
//             dev_type = devType.iOS

//     return conn_id, dev_type

// def parseObex(dataIn: bytes):
//     type = dataIn[0]
//     length = int.from_bytes(dataIn[1:3])
//     data = dataIn
//     return type, length, data

// # On Android, you can refer to an image by a handle to get the whole image. This allows you to pull old and current images.
// # After sending a request, you can just keep reading the socket until you reach the last image chunk to get a whole image.

// def getAndroidImage(sock, conn_id, handle: int):
//     print("Getting image handle:", handle)
//     # buffer = bytes.fromhex("83002dcb00000001420010782d62742f696d672d74686d0030001300") # OBEX GET x-bt/img-thm - returns 200x200 jpg on android
//     # buffer = bytes.fromhex("83002DCB00000001420010782D62742F696D672D696D670030001300") # OBEX GET - x-bt/img-img - returns same as above on android
//     buffer = bytes.fromhex("83002dcb" + conn_id.hex() + "420010782d62742f696d672d74686d0030001300")

//     # Images are referred to by handle
//     num_str = str(handle).zfill(7) # Put handle in the format of xxxxxxx. Example: 7 = 0000007
//     buffer += bytes.fromhex(''.join([f'{ord(digit):02x}00' for digit in num_str])) # Put 0x00 in etween each character

//     buffer += bytes.fromhex("009701") # Last chunk of file request

//     sock.send(buffer) # Send image request
//     finished = False
//     imageOpened = False
//     while not finished:
//         data_received = sock.recv(1025)  # Receive up to 1025 bytes

//         if data_received.hex() == ("c4000acb" + conn_id.hex() +"9701"): # Response from android when file not found
//             print(num_str + ": File not found")
//             finished = True
//             break
//         else:
//             if not imageOpened:
//                 fileName = str(num_str) + ".jpg"
//                 out_image = open(fileName, "wb")
//                 imageOpened = True

//         if data_received[0] == 144: # 0x90/144 is OBEX CONTINUE, aka. keep reading file
//             if data_received[8:10] == bytes.fromhex("9701"): # 0x97 0x01 enables OBEX SRM mode. Also marks first chunk of image
//                 print("start")
//                 out_image.write(data_received[13:])
//             else:
//                 out_image.write(data_received[11:])
//         elif data_received[0] == 160: # 0xa0/160 is OBEX SUCCESS, aka. stop reading file
//             out_image.write(data_received[11:])
//             finished = True
//             out_image.close()
//             outSize = os.path.getsize(fileName)
//             print(num_str, ": Saved", outSize, "bytes as", fileName)

// # iOS is weird. You just request any handle over and over again
// # to get all of the chunks of the current image. You can not request previous images.
// def getiOSImage(sock, conn_id):
//     handle = 0
//     print("Getting current image")
//     # buffer = bytes.fromhex("83002dcb00000001420010782d62742f696d672d74686d0030001300") # OBEX GET x-bt/img-thm - returns 200x200 jpg on android
//     # buffer = bytes.fromhex("83002DCB00000001420010782D62742F696D672D696D670030001300") # OBEX GET - x-bt/img-img - returns same as above on android
//     buffer = bytes.fromhex("83002dcb" + conn_id.hex() + "420010782d62742f696d672d74686d0030001300")
//     finished = False

//     # Images are referred to by handle
//     num_str = str(handle).zfill(7) # Put handle in the format of xxxxxxx. Example: 7 = 0000007
//     buffer  += bytes.fromhex(''.join([f'{ord(digit):02x}00' for digit in num_str])) # Put 0x00 in between each character

//     buffer += bytes.fromhex("009701") # Last chunk of file request

//     finished = False
//     outFile = open("iOS.jpg", "wb")
//     while not finished:
//         sock.send(buffer)
//         data_received = sock.recv(1025)  # Receive up to 1025 bytes
//         if data_received[0] == 144: # 0x90/144 is OBEX CONTINUE, aka. keep reading file
//             outFile.write(data_received[6:])

//         elif data_received[0] == 160: # 0xa0/160 is OBEX SUCCESS, aka. stop reading file
//             outFile.write(data_received[6:])
//             finished = True
//             outFile.close()
//             outSize = os.path.getsize("iOS.jpg")
//             print(num_str, ": Saved", outSize, "bytes as", "iOS.jpg")

const OBEX_HELLO: [u8; 26] = [
  0x80, 0x00, 0x1a, 0x15, 0x00, 0x06, 0x9b, 0x46, 0x00, 0x13, 0x71, 0x63, 0xdd, 0x54, 0x4a, 0x7e, 0x11, 0xe2, 0xb4,
  0x7c, 0x00, 0x50, 0xc2, 0x49, 0x00, 0x48,
];

fn create_l2cap_opts() -> Opts {
  let mut opts = Opts::default();
  opts.omtu = 1024;
  opts.imtu = 1024;
  opts.flush_to = 65535;
  opts.mode = 3;
  opts.fcs = 1;
  opts.max_tx = 16;
  opts.txwin_size = 63;

  opts
}

#[tokio::main]
async fn main() -> bluer::Result<()> {
  let session = bluer::Session::new().await?;
  let adapter = session.adapter("hci0")?;
  adapter.set_powered(true).await?;
  let target_sa = SocketAddr::new("C8:1F:E8:0F:C1:68".parse().unwrap(), AddressType::BrEdr, 0x1007);

  println!("Connecting to {:?}", &target_sa);
  let socket = Socket::new_stream()?;
  socket.set_l2cap_opts(&create_l2cap_opts())?;

  let mut stream = socket.connect(target_sa).await?;
  println!("Local address: {:?}", stream.as_ref().local_addr()?);
  println!("Remote address: {:?}", stream.peer_addr()?);
  println!("Send MTU: {:?}", stream.as_ref().send_mtu()?);
  println!("Recv MTU: {}", stream.as_ref().recv_mtu()?);
  println!("Security: {:?}", stream.as_ref().security()?);
  println!("Flow control: {:?}", stream.as_ref().flow_control()?);
  println!("L2CAP Options: {:?}", stream.as_ref().l2cap_opts()?);

  stream.write_all(&OBEX_HELLO).await?;
  let mut buffer = vec![0; 1024];
  let n = stream.read(&mut buffer).await?;
  println!("Received: {:?}", &buffer[..n]);
  let mut conn_id: [u8; 4] = [0, 0, 0, 0];

  if buffer[0] != 0xA0 {
    eprintln!("OBEX Handshake failed.");
    panic!("OBEX Handshake failed.");
  } else {
    conn_id.copy_from_slice(&buffer[8..12]);
    println!("Connection ID: {:?}", conn_id);
  }

  Ok(())
}
