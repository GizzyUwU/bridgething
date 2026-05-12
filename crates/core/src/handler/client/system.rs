use libbridgething::{
  Diagnostics,
  client::{
    ClientToBridgeSystemMsg, DeviceGetNickname, DeviceNicknameReply, DiagnosticsGet, DiagnosticsReply, LogsSubscribe,
    LogsSubscribeReply, LogsTail, LogsTailReply, LogsUnsubscribe, RequestVersion,
  },
};
use uuid::Uuid;

use super::{HandlerResult, MsgHandle};
use crate::systemd::power;

pub struct SystemHandler {
  handle: MsgHandle,
}

impl SystemHandler {
  pub fn new(handle: MsgHandle) -> Self {
    Self { handle }
  }

  pub async fn handle(&mut self, msg: ClientToBridgeSystemMsg) -> HandlerResult {
    tracing::debug!("({}) handling system message", &self.handle.from);

    match msg {
      ClientToBridgeSystemMsg::VersionRequest => self.version_request().await,
      ClientToBridgeSystemMsg::DiagnosticsGet => self.diagnostics_get().await,
      ClientToBridgeSystemMsg::LogsTail(req) => self.logs_tail(req).await,
      ClientToBridgeSystemMsg::LogsSubscribe(req) => self.logs_subscribe(req).await,
      ClientToBridgeSystemMsg::LogsUnsubscribe(req) => self.logs_unsubscribe(req).await,
      ClientToBridgeSystemMsg::Reboot => self.reboot().await,
      ClientToBridgeSystemMsg::PowerOff => self.power_off().await,
      ClientToBridgeSystemMsg::FactoryReset => self.factory_reset().await,
      ClientToBridgeSystemMsg::DeviceGetNickname => self.device_get_nickname().await,
    }
  }

  async fn device_get_nickname(&self) -> HandlerResult {
    let nickname = self.handle.state.meta.nickname();
    Ok(
      self
        .handle
        .respond_to::<DeviceGetNickname>(DeviceNicknameReply { nickname })
        .await?,
    )
  }

  async fn version_request(&self) -> HandlerResult {
    Ok(
      self
        .handle
        .respond_to::<RequestVersion>(self.handle.state.meta.snapshot())
        .await?,
    )
  }

  async fn diagnostics_get(&self) -> HandlerResult {
    let diagnostics = collect_diagnostics(&self.handle.state.meta.static_meta().version).await;
    Ok(
      self
        .handle
        .respond_to::<DiagnosticsGet>(DiagnosticsReply { diagnostics })
        .await?,
    )
  }

  async fn logs_tail(&self, req: LogsTail) -> HandlerResult {
    let entries = self
      .handle
      .state
      .log_tap
      .tail(req.source, &req.levels, req.filter.as_deref(), req.max_lines);
    Ok(self.handle.respond_to::<LogsTail>(LogsTailReply { entries }).await?)
  }

  async fn logs_subscribe(&self, req: LogsSubscribe) -> HandlerResult {
    let token = self.handle.state.log_tap.subscribe(
      self.handle.state.bus.clone(),
      self.handle.from,
      req.source,
      req.levels,
      req.filter,
    );
    Ok(
      self
        .handle
        .respond_to::<LogsSubscribe>(LogsSubscribeReply {
          token: token.to_string(),
        })
        .await?,
    )
  }

  async fn logs_unsubscribe(&self, req: LogsUnsubscribe) -> HandlerResult {
    let LogsUnsubscribe { token } = req;
    let Ok(uuid) = Uuid::parse_str(&token) else {
      tracing::trace!(%token, "system.logsUnsubscribe with malformed token; dropping");
      return Ok(());
    };
    if !self.handle.state.log_tap.unsubscribe(uuid) {
      tracing::trace!(%token, "system.logsUnsubscribe with unknown token; dropping");
    }
    Ok(())
  }

  async fn reboot(&self) -> HandlerResult {
    if let Err(err) = power::reboot().await {
      tracing::error!("reboot failed: {err}");
    }
    Ok(())
  }

  async fn power_off(&self) -> HandlerResult {
    if let Err(err) = power::power_off().await {
      tracing::error!("power_off failed: {err}");
    }
    Ok(())
  }

  async fn factory_reset(&mut self) -> HandlerResult {
    if let Err(err) = self.handle.bluetooth.profile_man.reset().await {
      tracing::error!("error resetting bluetooth devices: {:?}", err);
    }

    if let Err(err) = self.handle.state.reset().await {
      tracing::error!("error resetting state: {:?}", err);
    }

    self.reboot().await?;

    Ok(())
  }
}

async fn collect_diagnostics(daemon_version: &str) -> Diagnostics {
  let (mem_used_bytes, mem_avail_bytes) = read_meminfo().unwrap_or((0, 0));
  let (disk_used_bytes, disk_free_bytes) = read_disk_usage().unwrap_or((0, 0));
  Diagnostics {
    disk_used_bytes,
    disk_free_bytes,
    mem_used_bytes,
    mem_avail_bytes,
    uptime_s: read_uptime_s().unwrap_or(0),
    soc_temp_c: read_soc_temp_c(),
    load_avg: read_loadavg().unwrap_or([0.0, 0.0, 0.0]),
    daemon_version: daemon_version.to_string(),
    kernel_version: read_kernel_version().unwrap_or_default(),
    boot_id: read_boot_id().unwrap_or_default(),
  }
}

fn read_uptime_s() -> Option<u32> {
  let raw = std::fs::read_to_string("/proc/uptime").ok()?;
  let head = raw.split_whitespace().next()?;
  let secs: f64 = head.parse().ok()?;
  Some(secs as u32)
}

fn read_meminfo() -> Option<(u32, u32)> {
  let raw = std::fs::read_to_string("/proc/meminfo").ok()?;
  let mut total_kb: Option<u64> = None;
  let mut avail_kb: Option<u64> = None;
  for line in raw.lines() {
    if let Some(rest) = line.strip_prefix("MemTotal:") {
      total_kb = parse_kb(rest);
    } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
      avail_kb = parse_kb(rest);
    }
    if total_kb.is_some() && avail_kb.is_some() {
      break;
    }
  }
  let total = total_kb? * 1024;
  let avail = avail_kb? * 1024;
  let used = total.saturating_sub(avail);
  Some((
    u32::try_from(used).unwrap_or(u32::MAX),
    u32::try_from(avail).unwrap_or(u32::MAX),
  ))
}

fn parse_kb(rest: &str) -> Option<u64> {
  let mut tokens = rest.split_whitespace();
  let n: u64 = tokens.next()?.parse().ok()?;
  Some(n)
}

fn read_loadavg() -> Option<[f32; 3]> {
  let raw = std::fs::read_to_string("/proc/loadavg").ok()?;
  let mut tokens = raw.split_whitespace();
  let one: f32 = tokens.next()?.parse().ok()?;
  let five: f32 = tokens.next()?.parse().ok()?;
  let fifteen: f32 = tokens.next()?.parse().ok()?;
  Some([one, five, fifteen])
}

fn read_soc_temp_c() -> Option<f32> {
  let raw = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
  let milli: i32 = raw.trim().parse().ok()?;
  Some(milli as f32 / 1000.0)
}

fn read_disk_usage() -> Option<(u32, u32)> {
  let path = std::ffi::CString::new("/var").ok()?;
  // SAFETY: statvfs takes a NUL-terminated path and writes a stable
  // POSIX struct; both pointers are local stack allocations.
  let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
  let rc = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
  if rc != 0 {
    return None;
  }
  let block = stat.f_frsize as u64;
  let total = stat.f_blocks as u64 * block;
  let free = stat.f_bavail as u64 * block;
  let used = total.saturating_sub(free);
  Some((
    u32::try_from(used).unwrap_or(u32::MAX),
    u32::try_from(free).unwrap_or(u32::MAX),
  ))
}

fn read_kernel_version() -> Option<String> {
  let raw = std::fs::read_to_string("/proc/sys/kernel/osrelease").ok()?;
  Some(raw.trim().to_string())
}

fn read_boot_id() -> Option<String> {
  let raw = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
  Some(raw.trim().to_string())
}
