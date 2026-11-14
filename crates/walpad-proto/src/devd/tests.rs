use super::*;

#[test]
fn grammar() {
    assert_eq!(
        parse_devd_line("screen toggle"),
        Ok(Some(Command::Screen {
            mode: ScreenMode::Toggle
        }))
    );
    assert_eq!(
        parse_devd_line("rotate 270"),
        Ok(Some(Command::Rotate {
            mode: RotateMode::R270
        }))
    );
    assert_eq!(
        parse_devd_line("brightness +40"),
        Ok(Some(Command::SetBrightness(BrightnessArgs {
            value: None,
            delta: Some(40)
        })))
    );
    assert_eq!(
        parse_devd_line("brightness -40"),
        Ok(Some(Command::SetBrightness(BrightnessArgs {
            value: None,
            delta: Some(-40)
        })))
    );
    assert_eq!(
        parse_devd_line("brightness 90"),
        Ok(Some(Command::SetBrightness(BrightnessArgs {
            value: Some(90),
            delta: None
        })))
    );
    assert_eq!(
        parse_devd_line("keyboard place"),
        Ok(Some(Command::Keyboard {
            mode: KeyboardMode::Toggle
        }))
    );
    assert_eq!(parse_devd_line("reload"), Ok(None));
    assert_eq!(parse_devd_line("   "), Ok(None));
    assert!(parse_devd_line("brightness 999").is_err());
    assert!(parse_devd_line("brightness x").is_err());
    assert!(parse_devd_line("rotate 45").is_err());
    assert!(parse_devd_line("rm -rf /").is_err());
}
