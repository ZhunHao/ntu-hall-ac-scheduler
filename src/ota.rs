//! Authorization gate for `POST /update` (OTA firmware upload).
//!
//! The token only proves the uploader knows the shared secret; image authenticity is
//! enforced separately by ESP-IDF's signed-app check inside `esp_ota_end`
//! (`CONFIG_SECURE_SIGNED_ON_UPDATE_NO_SECURE_BOOT`, see sdkconfig.defaults).

/// Request header carrying the OTA token.
pub const OTA_TOKEN_HEADER: &str = "X-OTA-Token";

/// Shorter tokens are treated as unset, so a weak secret disables OTA instead of guarding it.
pub const MIN_TOKEN_LEN: usize = 16;

/// Placeholder shipped in cfg.toml.example; never accepted as a real token.
pub const PLACEHOLDER_TOKEN: &str = "change-me-to-a-long-random-string";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaRejection {
    /// No usable `[ota] token` in cfg.toml.
    Disabled,
    /// The board is serving its fallback AP, whose password is a known default.
    FallbackApMode,
    MissingToken,
    WrongToken,
}

impl OtaRejection {
    pub fn status_code(self) -> u16 {
        match self {
            OtaRejection::Disabled | OtaRejection::FallbackApMode => 403,
            OtaRejection::MissingToken | OtaRejection::WrongToken => 401,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            OtaRejection::Disabled => "OTA is disabled: set [ota] token in cfg.toml",
            OtaRejection::FallbackApMode => "OTA is disabled while in fallback AP mode",
            OtaRejection::MissingToken => "Missing X-OTA-Token header",
            OtaRejection::WrongToken => "Invalid OTA token",
        }
    }
}

/// Returns the configured token if it is strong enough to guard OTA.
pub fn usable_token(configured: Option<&str>) -> Option<&str> {
    configured.filter(|t| t.len() >= MIN_TOKEN_LEN && *t != PLACEHOLDER_TOKEN)
}

/// Decides whether an OTA upload may start. Runs before the OTA partition is erased.
pub fn authorize_ota(
    configured: Option<&str>,
    presented: Option<&str>,
    in_ap_mode: bool,
) -> Result<(), OtaRejection> {
    let expected = usable_token(configured).ok_or(OtaRejection::Disabled)?;
    if in_ap_mode {
        return Err(OtaRejection::FallbackApMode);
    }
    let presented = presented.ok_or(OtaRejection::MissingToken)?;
    if constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
        Ok(())
    } else {
        Err(OtaRejection::WrongToken)
    }
}

/// Compares two byte strings without an early exit on the first mismatch.
/// Run time depends only on the longer length, so it leaks length but not content.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let len = a.len().max(b.len());
    let mut diff = u8::from(a.len() != b.len());
    for i in 0..len {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}
