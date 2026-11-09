use crate::{BrightnessArgs, Command, KeyboardMode, RotateMode, ScreenMode};

/// Parse one line of the legacy `/run/walpad/devd.cmd` grammar:
/// `screen on|off|toggle`, `rotate auto|lock|toggle|0|90|180|270`, `brightness N|+N|-N`,
/// `keyboard [place]`, `reload` (returns Ok(None): handled by the caller).
pub fn parse_devd_line(line: &str) -> Result<Option<Command>, String> {
    let p: Vec<&str> = line.split_whitespace().collect();
    let Some(&verb) = p.first() else {
        return Ok(None);
    };
    let arg = p.get(1).copied();
    let cmd = match (verb, arg) {
        ("screen", Some(m)) => Command::Screen {
            mode: match m {
                "on" => ScreenMode::On,
                "off" => ScreenMode::Off,
                "toggle" => ScreenMode::Toggle,
                _ => return Err(format!("bad screen mode {m:?}")),
            },
        },
        ("rotate", Some(m)) => Command::Rotate {
            mode: match m {
                "auto" => RotateMode::Auto,
                "lock" => RotateMode::Lock,
                "toggle" => RotateMode::Toggle,
                "0" => RotateMode::R0,
                "90" => RotateMode::R90,
                "180" => RotateMode::R180,
                "270" => RotateMode::R270,
                _ => return Err(format!("bad rotate mode {m:?}")),
            },
        },
        ("brightness", Some(a)) => {
            let n: i32 = a.parse().map_err(|_| format!("bad brightness {a:?}"))?;
            let args = if a.starts_with('+') || a.starts_with('-') {
                BrightnessArgs {
                    value: None,
                    delta: Some(n),
                }
            } else {
                BrightnessArgs {
                    value: Some(n),
                    delta: None,
                }
            };
            Command::SetBrightness(args)
        }
        ("keyboard", _) => Command::Keyboard {
            mode: KeyboardMode::Toggle,
        },
        ("reload", _) => return Ok(None),
        _ => return Err(format!("unknown devd command {verb:?}")),
    };
    cmd.validate()?;
    Ok(Some(cmd))
}

#[cfg(test)]
mod tests {
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
}
