//! credit to the librespot project

use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use librespot_protocol::{
  authentication::{APWelcome, AuthenticationType, ClientResponseEncrypted, CpuFamily, Os},
  keyexchange::{
    APLoginFailed, APResponseMessage, ClientHello, ClientResponsePlaintext, Cryptosuite, Platform, Product,
    ProductFlags,
  },
};
use num_bigint::BigUint;
use protobuf::Message;
use serde_json::Value;
use sha1::{Digest, Sha1};
use shannon::Shannon;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpStream,
};

use crate::{
  auth::Auth,
  error::{Error, Result},
};

const SPOTIFY_VERSION: u64 = 124200290;
const LOGIN: u8 = 0xab;
const AP_WELCOME: u8 = 0xac;
const AUTH_FAILURE: u8 = 0xad;

const SERVER_KEY: [u8; 256] = [
  0xac, 0xe0, 0x46, 0x0b, 0xff, 0xc2, 0x30, 0xaf, 0xf4, 0x6b, 0xfe, 0xc3, 0xbf, 0xbf, 0x86, 0x3d, 0xa1, 0x91, 0xc6,
  0xcc, 0x33, 0x6c, 0x93, 0xa1, 0x4f, 0xb3, 0xb0, 0x16, 0x12, 0xac, 0xac, 0x6a, 0xf1, 0x80, 0xe7, 0xf6, 0x14, 0xd9,
  0x42, 0x9d, 0xbe, 0x2e, 0x34, 0x66, 0x43, 0xe3, 0x62, 0xd2, 0x32, 0x7a, 0x1a, 0x0d, 0x92, 0x3b, 0xae, 0xdd, 0x14,
  0x02, 0xb1, 0x81, 0x55, 0x05, 0x61, 0x04, 0xd5, 0x2c, 0x96, 0xa4, 0x4c, 0x1e, 0xcc, 0x02, 0x4a, 0xd4, 0xb2, 0x0c,
  0x00, 0x1f, 0x17, 0xed, 0xc2, 0x2f, 0xc4, 0x35, 0x21, 0xc8, 0xf0, 0xcb, 0xae, 0xd2, 0xad, 0xd7, 0x2b, 0x0f, 0x9d,
  0xb3, 0xc5, 0x32, 0x1a, 0x2a, 0xfe, 0x59, 0xf3, 0x5a, 0x0d, 0xac, 0x68, 0xf1, 0xfa, 0x62, 0x1e, 0xfb, 0x2c, 0x8d,
  0x0c, 0xb7, 0x39, 0x2d, 0x92, 0x47, 0xe3, 0xd7, 0x35, 0x1a, 0x6d, 0xbd, 0x24, 0xc2, 0xae, 0x25, 0x5b, 0x88, 0xff,
  0xab, 0x73, 0x29, 0x8a, 0x0b, 0xcc, 0xcd, 0x0c, 0x58, 0x67, 0x31, 0x89, 0xe8, 0xbd, 0x34, 0x80, 0x78, 0x4a, 0x5f,
  0xc9, 0x6b, 0x89, 0x9d, 0x95, 0x6b, 0xfc, 0x86, 0xd7, 0x4f, 0x33, 0xa6, 0x78, 0x17, 0x96, 0xc9, 0xc3, 0x2d, 0x0d,
  0x32, 0xa5, 0xab, 0xcd, 0x05, 0x27, 0xe2, 0xf7, 0x10, 0xa3, 0x96, 0x13, 0xc4, 0x2f, 0x99, 0xc0, 0x27, 0xbf, 0xed,
  0x04, 0x9c, 0x3c, 0x27, 0x58, 0x04, 0xb6, 0xb2, 0x19, 0xf9, 0xc1, 0x2f, 0x02, 0xe9, 0x48, 0x63, 0xec, 0xa1, 0xb6,
  0x42, 0xa0, 0x9d, 0x48, 0x25, 0xf8, 0xb3, 0x9d, 0xd0, 0xe8, 0x6a, 0xf9, 0x48, 0x4d, 0xa1, 0xc2, 0xba, 0x86, 0x30,
  0x42, 0xea, 0x9d, 0xb3, 0x08, 0x6c, 0x19, 0x0e, 0x48, 0xb3, 0x9d, 0x66, 0xeb, 0x00, 0x06, 0xa2, 0x5a, 0xee, 0xa1,
  0x1b, 0x13, 0x87, 0x3c, 0xd7, 0x19, 0xe6, 0x55, 0xbd,
];

const DH_PRIME: [u8; 96] = [
  0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xc9, 0x0f, 0xda, 0xa2, 0x21, 0x68, 0xc2, 0x34, 0xc4, 0xc6, 0x62,
  0x8b, 0x80, 0xdc, 0x1c, 0xd1, 0x29, 0x02, 0x4e, 0x08, 0x8a, 0x67, 0xcc, 0x74, 0x02, 0x0b, 0xbe, 0xa6, 0x3b, 0x13,
  0x9b, 0x22, 0x51, 0x4a, 0x08, 0x79, 0x8e, 0x34, 0x04, 0xdd, 0xef, 0x95, 0x19, 0xb3, 0xcd, 0x3a, 0x43, 0x1b, 0x30,
  0x2b, 0x0a, 0x6d, 0xf2, 0x5f, 0x14, 0x37, 0x4f, 0xe1, 0x35, 0x6d, 0x6d, 0x51, 0xc2, 0x45, 0xe4, 0x85, 0xb5, 0x76,
  0x62, 0x5e, 0x7e, 0xc6, 0xf4, 0x4c, 0x42, 0xe9, 0xa6, 0x3a, 0x36, 0x20, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
  0xff,
];

type HmacSha1 = Hmac<Sha1>;

fn rand_bytes(n: usize) -> Vec<u8> {
  (0..n).map(|_| rand::random::<u8>()).collect()
}

pub async fn resolve_and_cache(auth: &Auth, http: &reqwest::Client, device_id: &str) -> Result<String> {
  if let Some(u) = auth.store().load_username() {
    return Ok(u);
  }
  let bearer = auth.bearer().await?;
  let username = resolve_username(http, &bearer, device_id).await?;
  auth.store().save_username(username.clone());
  Ok(username)
}

const AP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn resolve_username(http: &reqwest::Client, access_token: &str, device_id: &str) -> Result<String> {
  tokio::time::timeout(AP_HANDSHAKE_TIMEOUT, async {
    let (host, port) = apresolve(http).await?;
    let sock = TcpStream::connect((host.as_str(), port)).await.map_err(Error::other)?;
    let mut conn = handshake(sock).await?;
    authenticate(&mut conn, access_token, device_id).await
  })
  .await
  .map_err(|_| Error::other("ap username resolution timed out"))?
}

async fn apresolve(http: &reqwest::Client) -> Result<(String, u16)> {
  let v: Value = http
    .get("https://apresolve.spotify.com/")
    .query(&[("type", "accesspoint")])
    .send()
    .await?
    .json()
    .await?;
  let entry = v["accesspoint"][0]
    .as_str()
    .ok_or_else(|| Error::other("apresolve returned no accesspoint"))?;
  let (host, port) = entry
    .rsplit_once(':')
    .ok_or_else(|| Error::other("malformed accesspoint host"))?;
  Ok((host.to_string(), port.parse().map_err(Error::other)?))
}

struct ApConn {
  sock: TcpStream,
  send: Shannon,
  recv: Shannon,
  encode_nonce: u32,
  decode_nonce: u32,
}

impl ApConn {
  async fn send_packet(&mut self, cmd: u8, payload: &[u8]) -> Result<()> {
    let mut buf = Vec::with_capacity(3 + payload.len());
    buf.push(cmd);
    buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    buf.extend_from_slice(payload);
    self.send.nonce_u32(self.encode_nonce);
    self.encode_nonce += 1;
    self.send.encrypt(&mut buf);
    let mut mac = [0u8; 4];
    self.send.finish(&mut mac);
    buf.extend_from_slice(&mac);
    self.sock.write_all(&buf).await.map_err(Error::other)
  }

  async fn recv_packet(&mut self) -> Result<(u8, Vec<u8>)> {
    let mut header = [0u8; 3];
    self.sock.read_exact(&mut header).await.map_err(Error::other)?;
    self.recv.nonce_u32(self.decode_nonce);
    self.decode_nonce += 1;
    self.recv.decrypt(&mut header);
    let cmd = header[0];
    let size = u16::from_be_bytes([header[1], header[2]]) as usize;
    let mut payload = vec![0u8; size + 4];
    self.sock.read_exact(&mut payload).await.map_err(Error::other)?;
    self.recv.decrypt(&mut payload[..size]);
    let mac = payload.split_off(size);
    self.recv.check_mac(&mac).map_err(Error::other)?;
    Ok((cmd, payload))
  }
}

async fn handshake(mut sock: TcpStream) -> Result<ApConn> {
  let prime = BigUint::from_bytes_be(&DH_PRIME);
  let generator = BigUint::from(2u32);
  let private = BigUint::from_bytes_le(&rand_bytes(95));
  let gc = generator.modpow(&private, &prime).to_bytes_be();

  let hello = client_hello(gc);
  let mut acc = vec![0u8, 4];
  let size = 2 + 4 + hello.compute_size();
  acc.extend_from_slice(&(size as u32).to_be_bytes());
  hello.write_to_vec(&mut acc)?;
  sock.write_all(&acc).await.map_err(Error::other)?;

  let mut len_buf = [0u8; 4];
  sock.read_exact(&mut len_buf).await.map_err(Error::other)?;
  acc.extend_from_slice(&len_buf);
  let resp_size = u32::from_be_bytes(len_buf) as usize;
  let body_len = resp_size
    .checked_sub(4)
    .ok_or_else(|| Error::other("AP response size too small"))?;
  let mut resp_bytes = vec![0u8; body_len];
  sock.read_exact(&mut resp_bytes).await.map_err(Error::other)?;
  acc.extend_from_slice(&resp_bytes);

  let resp = APResponseMessage::parse_from_bytes(&resp_bytes)?;
  let dh = resp.challenge.login_crypto_challenge.diffie_hellman.clone();
  let remote_key = dh.gs().to_vec();
  let remote_sig = dh.gs_signature().to_vec();

  if !verify_server_signature(&remote_key, &remote_sig) {
    return Err(Error::Auth("AP server-key signature verification failed".into()));
  }

  let shared = BigUint::from_bytes_be(&remote_key)
    .modpow(&private, &prime)
    .to_bytes_be();
  let (challenge, send_key, recv_key) = compute_keys(&shared, &acc)?;

  let mut response = ClientResponsePlaintext::new();
  response
    .login_crypto_response
    .mut_or_insert_default()
    .diffie_hellman
    .mut_or_insert_default()
    .set_hmac(challenge);
  response.pow_response.mut_or_insert_default();
  response.crypto_response.mut_or_insert_default();

  let mut out = Vec::new();
  let size = 4 + response.compute_size();
  out.extend_from_slice(&(size as u32).to_be_bytes());
  response.write_to_vec(&mut out)?;
  sock.write_all(&out).await.map_err(Error::other)?;

  Ok(ApConn {
    sock,
    send: Shannon::new(&send_key),
    recv: Shannon::new(&recv_key),
    encode_nonce: 0,
    decode_nonce: 0,
  })
}

fn client_hello(gc: Vec<u8>) -> ClientHello {
  let mut p = ClientHello::new();
  {
    let bi = p.build_info.mut_or_insert_default();
    bi.set_product(Product::PRODUCT_CLIENT);
    bi.product_flags.push(ProductFlags::PRODUCT_FLAG_NONE.into());
    bi.set_platform(Platform::PLATFORM_LINUX_X86_64);
    bi.set_version(SPOTIFY_VERSION);
  }
  p.cryptosuites_supported.push(Cryptosuite::CRYPTO_SUITE_SHANNON.into());
  {
    let dh = p
      .login_crypto_hello
      .mut_or_insert_default()
      .diffie_hellman
      .mut_or_insert_default();
    dh.set_gc(gc);
    dh.set_server_keys_known(1);
  }
  p.set_client_nonce(rand_bytes(0x10));
  p.set_padding(vec![0x1e]);
  p
}

async fn authenticate(conn: &mut ApConn, access_token: &str, device_id: &str) -> Result<String> {
  let mut packet = ClientResponseEncrypted::new();
  {
    let lc = packet.login_credentials.mut_or_insert_default();
    lc.set_typ(AuthenticationType::AUTHENTICATION_SPOTIFY_TOKEN);
    lc.set_auth_data(access_token.as_bytes().to_vec());
  }
  {
    let si = packet.system_info.mut_or_insert_default();
    si.set_cpu_family(CpuFamily::CPU_X86_64);
    si.set_os(Os::OS_LINUX);
    si.set_system_information_string("bridgething-sfp".to_string());
    si.set_device_id(device_id.to_string());
  }
  packet.set_version_string("bridgething-sfp 0.1".to_string());

  conn.send_packet(LOGIN, &packet.write_to_bytes()?).await?;
  let (cmd, data) = conn.recv_packet().await?;
  match cmd {
    AP_WELCOME => Ok(APWelcome::parse_from_bytes(&data)?.canonical_username().to_string()),
    AUTH_FAILURE => {
      let f = APLoginFailed::parse_from_bytes(&data)?;
      Err(Error::Auth(format!("AP login failed: {:?}", f.error_code())))
    }
    other => Err(Error::other(format!("unexpected AP packet 0x{other:02x}"))),
  }
}

fn compute_keys(shared: &[u8], packets: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
  let mut data = Vec::with_capacity(0x64);
  for i in 1u8..6 {
    let mut mac = HmacSha1::new_from_slice(shared).map_err(Error::other)?;
    mac.update(packets);
    mac.update(&[i]);
    data.extend_from_slice(&mac.finalize().into_bytes());
  }
  let mut mac = HmacSha1::new_from_slice(&data[..0x14]).map_err(Error::other)?;
  mac.update(packets);
  Ok((
    mac.finalize().into_bytes().to_vec(),
    data[0x14..0x34].to_vec(),
    data[0x34..0x54].to_vec(),
  ))
}

fn verify_server_signature(gs: &[u8], sig: &[u8]) -> bool {
  const SHA1_DIGESTINFO: [u8; 15] = [
    0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14,
  ];
  let n = BigUint::from_bytes_be(&SERVER_KEY);
  let e = BigUint::from(65537u32);
  let m = BigUint::from_bytes_be(sig).modpow(&e, &n);
  let mut em = m.to_bytes_be();
  if em.len() > 256 {
    return false;
  }
  if em.len() < 256 {
    let mut padded = vec![0u8; 256 - em.len()];
    padded.extend_from_slice(&em);
    em = padded;
  }
  let hash = Sha1::digest(gs);
  let mut expect = Vec::with_capacity(256);
  expect.push(0x00);
  expect.push(0x01);
  let pad = 256 - 3 - SHA1_DIGESTINFO.len() - hash.len();
  expect.extend(std::iter::repeat_n(0xff, pad));
  expect.push(0x00);
  expect.extend_from_slice(&SHA1_DIGESTINFO);
  expect.extend_from_slice(&hash);
  em == expect
}
