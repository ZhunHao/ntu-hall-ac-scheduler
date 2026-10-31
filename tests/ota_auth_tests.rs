use ac_scheduler::config::AppConfig;
use ac_scheduler::ota::{
    authorize_ota, constant_time_eq, usable_token, OtaRejection, MIN_TOKEN_LEN, OTA_TOKEN_HEADER,
    PLACEHOLDER_TOKEN,
};

const TOKEN: &str = "3f9c1e7a5b2d4f60a8c9e1b7";

#[test]
fn correct_token_on_home_network_is_accepted() {
    assert_eq!(authorize_ota(Some(TOKEN), Some(TOKEN), false), Ok(()));
}

#[test]
fn missing_or_wrong_token_is_rejected_with_401() {
    for (presented, expected) in [
        (None, OtaRejection::MissingToken),
        (Some(""), OtaRejection::WrongToken),
        (Some("3f9c1e7a5b2d4f60a8c9e1b8"), OtaRejection::WrongToken),
        (Some("3f9c1e7a5b2d4f60a8c9e1b"), OtaRejection::WrongToken),
        (Some("3f9c1e7a5b2d4f60a8c9e1b77"), OtaRejection::WrongToken),
        (Some("3F9C1E7A5B2D4F60A8C9E1B7"), OtaRejection::WrongToken),
    ] {
        let res = authorize_ota(Some(TOKEN), presented, false);
        assert_eq!(res, Err(expected), "presented {presented:?}");
        assert_eq!(expected.status_code(), 401);
    }
}

#[test]
fn fallback_ap_mode_refuses_even_a_correct_token() {
    let res = authorize_ota(Some(TOKEN), Some(TOKEN), true);
    assert_eq!(res, Err(OtaRejection::FallbackApMode));
    assert_eq!(OtaRejection::FallbackApMode.status_code(), 403);
}

#[test]
fn ota_is_disabled_without_a_usable_configured_token() {
    let short = "a".repeat(MIN_TOKEN_LEN - 1);
    for configured in [
        None,
        Some(""),
        Some(short.as_str()),
        Some(PLACEHOLDER_TOKEN),
    ] {
        // A client that guesses the weak or placeholder value still gets nothing
        let res = authorize_ota(configured, configured, false);
        assert_eq!(
            res,
            Err(OtaRejection::Disabled),
            "configured {configured:?}"
        );
    }
    assert_eq!(OtaRejection::Disabled.status_code(), 403);
}

#[test]
fn minimum_length_token_is_usable() {
    let token = "a".repeat(MIN_TOKEN_LEN);
    assert_eq!(usable_token(Some(&token)), Some(token.as_str()));
}

#[test]
fn constant_time_eq_matches_slice_equality() {
    let cases: [(&[u8], &[u8]); 6] = [
        (b"", b""),
        (b"abc", b"abc"),
        (b"abc", b"abd"),
        (b"abc", b"ab"),
        (b"ab", b"ab\0"),
        (b"", b"\0"),
    ];
    for (a, b) in cases {
        assert_eq!(constant_time_eq(a, b), a == b, "{a:?} vs {b:?}");
    }
}

#[test]
fn config_without_ota_section_still_parses_with_ota_disabled() {
    let cfg: AppConfig = toml::from_str(
        r#"
        [wifi]
        ssid = "s"
        password = "p"
        "#,
    )
    .unwrap();
    assert_eq!(cfg.ota.token, None);
    assert_eq!(
        authorize_ota(cfg.ota.token.as_deref(), Some(TOKEN), false),
        Err(OtaRejection::Disabled)
    );
}

#[test]
fn example_config_ships_the_placeholder_so_ota_starts_disabled() {
    let cfg: AppConfig = toml::from_str(include_str!("../cfg.toml.example")).unwrap();
    assert_eq!(cfg.ota.token.as_deref(), Some(PLACEHOLDER_TOKEN));
    assert_eq!(usable_token(cfg.ota.token.as_deref()), None);
}

#[test]
fn header_name_matches_the_web_ui() {
    assert_eq!(OTA_TOKEN_HEADER, "X-OTA-Token");
}
