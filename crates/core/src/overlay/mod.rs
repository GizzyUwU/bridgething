use std::sync::Arc;

use libbridgething::OverlayProfile;

const OVERLAY_JS: &str = include_str!("overlay.js");

pub fn kiosk_origin(modern_port: u16) -> String {
  format!("http://127.0.0.1:{modern_port}")
}

pub fn overlay_script(profile: &OverlayProfile, modern_port: u16, body: Option<&str>) -> Option<Arc<String>> {
  if !profile.any_enabled() {
    return None;
  }
  let config = serde_json::json!({
    "origin": kiosk_origin(modern_port),
    "surfaces": {
      "notifications": profile.notifications,
      "call": profile.call,
      "pairing": profile.pairing,
      "connection": profile.connection,
      "volume": profile.volume,
    },
  });
  let body = body.unwrap_or(OVERLAY_JS);
  Some(Arc::new(format!("window.__bridgethingOverlay = {config};\n{body}")))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn all_off_produces_no_script() {
    let off = OverlayProfile {
      notifications: false,
      call: false,
      pairing: false,
      connection: false,
      volume: false,
    };
    assert!(overlay_script(&off, 8891, None).is_none());
  }

  #[test]
  fn default_profile_injects_every_surface() {
    let script = overlay_script(&OverlayProfile::default(), 8891, None).expect("script");
    assert!(script.starts_with("window.__bridgethingOverlay = "));
    for surface in ["notifications", "call", "pairing", "connection", "volume"] {
      assert!(script.contains(&format!("\"{surface}\":true")), "{surface} on");
    }
    assert!(script.contains(&kiosk_origin(8891)));
  }

  #[test]
  fn partial_profile_carries_per_surface_flags() {
    let profile = OverlayProfile {
      notifications: true,
      call: false,
      pairing: true,
      connection: false,
      volume: false,
    };
    let script = overlay_script(&profile, 8891, None).expect("script");
    assert!(script.contains("\"notifications\":true"));
    assert!(script.contains("\"call\":false"));
    assert!(script.contains("\"pairing\":true"));
    assert!(script.contains("\"connection\":false"));
  }

  #[test]
  fn custom_body_replaces_the_builtin_under_the_same_prelude() {
    let profile = OverlayProfile::default();
    let script = overlay_script(&profile, 8891, Some("/* mine */")).expect("script");
    assert!(script.starts_with("window.__bridgethingOverlay = "));
    assert!(script.contains(&kiosk_origin(8891)));
    assert!(script.ends_with("/* mine */"));
    assert!(!script.contains("__bridgethingOverlayMounted"));
  }

  #[test]
  fn a_custom_body_still_honors_the_all_off_short_circuit() {
    let off = OverlayProfile {
      notifications: false,
      call: false,
      pairing: false,
      connection: false,
      volume: false,
    };
    assert!(overlay_script(&off, 8891, Some("/* mine */")).is_none());
  }
}
