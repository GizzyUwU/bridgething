use std::{env, process::ExitCode};

use bridgething_mfi::{MfiAuth, RemoteI2c};

fn main() -> ExitCode {
  let target = env::args().nth(1).unwrap_or_else(|| "127.0.0.1:9090".to_string());
  println!("connecting to {target}");

  let transport = match RemoteI2c::connect(target.as_str()) {
    Ok(t) => t,
    Err(e) => {
      eprintln!("connect failed: {e:?}");
      return ExitCode::FAILURE;
    }
  };
  let mut auth = MfiAuth::with_transport(transport);
  let mut ok = true;

  match auth.version() {
    Ok(v) => println!("version    : 0x{v:02x}"),
    Err(e) => {
      println!("version    : ERR {e:?}");
      ok = false;
    }
  }
  match auth.last_error() {
    Ok(v) => println!("last_error : 0x{v:02x}"),
    Err(e) => {
      println!("last_error : ERR {e:?}");
      ok = false;
    }
  }
  match auth.status() {
    Ok(v) => println!("status     : 0x{v:02x}"),
    Err(e) => {
      println!("status     : ERR {e:?}");
      ok = false;
    }
  }
  match auth.cert_len() {
    Ok(v) => println!("cert_len   : {v} bytes"),
    Err(e) => {
      println!("cert_len   : ERR {e:?}");
      ok = false;
    }
  }
  match auth.cert() {
    Ok(c) => println!(
      "cert       : {} bytes, head=[{}]",
      c.len(),
      c.iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" "),
    ),
    Err(e) => {
      println!("cert       : ERR {e:?}");
      ok = false;
    }
  }
  match auth.serial() {
    Ok(s) => println!(
      "serial     : [{}]",
      s.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
    ),
    Err(e) => {
      println!("serial     : ERR {e:?}");
      ok = false;
    }
  }

  if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
