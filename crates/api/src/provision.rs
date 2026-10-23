//! The provisioning blob `just provision` writes to the `cc_prov` flash partition:
//! settings applied once at the next boot, then erased (WiFi credentials for the
//! bench, a hostname, MQTT broker, ...).
//!
//! Format: the line `CCPROV1`, then one `key=value` line per parameter (keys as in
//! `/api/config`, values in their text form). Erased flash (`0xFF`) ends the blob.

use cc_core::Config;
use cc_core::config::SetError;

pub const MAGIC: &str = "CCPROV1\n";

/// The `key=value` pairs of a blob, or `None` when the partition holds no blob.
pub fn pairs(blob: &[u8]) -> Option<impl Iterator<Item = (&str, &str)>> {
    let end = blob
        .iter()
        .position(|&b| b == 0xFF || b == 0)
        .unwrap_or(blob.len());
    let text = core::str::from_utf8(&blob[..end]).ok()?;
    let body = text.strip_prefix(MAGIC)?;
    Some(
        body.lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim(), v)),
    )
}

/// Applies a blob; returns how many parameters were set and the first refused key.
pub fn apply<'b>(blob: &'b [u8], cfg: &mut Config) -> Option<(usize, Option<(&'b str, SetError)>)> {
    let mut set = 0;
    let mut first_error = None;
    for (k, v) in pairs(blob)? {
        match cfg.set_from_str(k, v) {
            Ok(_) => set += 1,
            Err(e) => {
                first_error.get_or_insert((k, e));
            }
        }
    }
    Some((set, first_error))
}

/// Builds a blob (used by `tools/provision`); every pair is validated first.
#[cfg(feature = "std")]
pub fn build<'a>(
    pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<std::vec::Vec<u8>, std::string::String> {
    let mut cfg = Config::default();
    let mut out = std::string::String::from(MAGIC);
    for (k, v) in pairs {
        if v.contains('\n') {
            return Err(std::format!("{k}: value must be on one line"));
        }
        cfg.set_from_str(k, v).map_err(|e| std::format!("{k}: {e}"))?;
        out.push_str(k);
        out.push('=');
        out.push_str(v);
        out.push('\n');
    }
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_pairs_until_erased_flash() {
        let mut blob = b"CCPROV1\nsystem.wifi.ssid=Bench\nsystem.wifi.password=a=b c\nnope=1\n".to_vec();
        blob.extend([0xFF; 32]);
        let mut cfg = Config::default();
        let (n, err) = apply(&blob, &mut cfg).unwrap();
        assert_eq!(n, 2);
        assert_eq!(err, Some(("nope", SetError::UnknownKey)));
        assert_eq!(cfg.system_wifi_ssid.as_str(), "Bench");
        assert_eq!(cfg.system_wifi_password.as_str(), "a=b c");
    }

    #[test]
    fn erased_partition_is_no_blob() {
        assert!(pairs(&[0xFF; 64]).is_none());
        assert!(pairs(b"garbage").is_none());
    }
}
