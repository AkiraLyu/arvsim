//! arvsim 命令行入口。
//!
//! 入口把参数解析、镜像装载和机器组装串联起来；CPU 执行由库中的 [`arvsim::machine::Machine`] 完成。
//! 参数或平台配置错误返回退出码 2，宿主装载失败或未被 guest trap 接管的异常返回退出码 1。

use arvsim::cfg;
use arvsim::loader::{self, ImageFormat};
use arvsim::machine::{DebugLevel, Platform, RunOptions, RunOutcome};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_MAX_STEPS: u64 = 1_000_000;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PlatformPreset {
    // `Bare` 仍有 DRAM，只是不挂载 UART MMIO 窗口。
    Bare,
    Uart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    image: PathBuf,
    format: ImageFormat,
    platform: PlatformPreset,
    dram_base: u64,
    dram_size: usize,
    uart_base: u64,
    entry: Option<u64>,
    max_steps: Option<u64>,
    debug: DebugLevel,
}

#[derive(Debug)]
enum Command {
    Help,
    Run(CliOptions),
}

fn main() -> ExitCode {
    let command = match parse_args(env::args().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("error: {message}\n\n{}", usage());
            return ExitCode::from(2);
        }
    };

    match command {
        Command::Help => {
            println!("{}", usage());
            ExitCode::SUCCESS
        }
        Command::Run(options) => run(options),
    }
}

fn run(options: CliOptions) -> ExitCode {
    // 平台层在分配内存和挂载设备前统一验证地址范围。
    let mut platform = match Platform::new(options.dram_base, options.dram_size) {
        Ok(platform) => platform,
        Err(error) => {
            eprintln!("error: invalid platform configuration: {error}");
            return ExitCode::from(2);
        }
    };
    let loaded = match loader::load_image(&mut platform.dram_mut(), &options.image, options.format)
    {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!(
                "error: failed to load image '{}': {error}",
                options.image.display()
            );
            return ExitCode::from(1);
        }
    };
    let entry = options.entry.unwrap_or(loaded.entry);
    if options.platform == PlatformPreset::Uart
        && let Err(error) = platform.attach_uart(options.uart_base)
    {
        eprintln!("error: invalid UART mapping: {error}");
        return ExitCode::from(2);
    }

    let mut machine = match platform.build(entry) {
        Ok(machine) => machine,
        Err(error) => {
            eprintln!("error: failed to build machine: {error}");
            return ExitCode::from(2);
        }
    };
    machine.reset();
    match machine.run(RunOptions {
        max_steps: options.max_steps,
        debug: options.debug,
    }) {
        RunOutcome::StepLimitReached { steps } => {
            println!(
                "stopped after {steps} steps at pc={:#x} (step limit reached)",
                machine.cpu.pc
            );
            ExitCode::SUCCESS
        }
        RunOutcome::Exception {
            steps,
            pc,
            exception,
        } => {
            eprintln!(
                "error: fatal CPU exception after {steps} steps at pc={pc:#x}: {exception:?}"
            );
            ExitCode::from(1)
        }
    }
}

fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut format = ImageFormat::Auto;
    let mut platform = PlatformPreset::Uart;
    let mut dram_base = cfg::DRAM_BASE;
    let mut dram_size = cfg::DRAM_SIZE;
    let mut uart_base = None;
    let mut entry = None;
    let mut max_steps = Some(DEFAULT_MAX_STEPS);
    let mut debug = DebugLevel::Off;
    let mut image = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--format" => {
                format = match next_value(&mut args, "--format")?.as_str() {
                    "auto" => ImageFormat::Auto,
                    "flat" => ImageFormat::Flat,
                    "elf" => ImageFormat::Elf,
                    value => return Err(format!("invalid image format: {value}")),
                }
            }
            "--platform" => {
                platform = match next_value(&mut args, "--platform")?.as_str() {
                    "bare" => PlatformPreset::Bare,
                    "uart" => PlatformPreset::Uart,
                    value => return Err(format!("invalid platform: {value}")),
                }
            }
            "--dram-base" => {
                dram_base = parse_number("--dram-base", &next_value(&mut args, "--dram-base")?)?
            }
            "--dram-size" => {
                dram_size = parse_size("--dram-size", &next_value(&mut args, "--dram-size")?)?
            }
            "--uart-base" => {
                uart_base = Some(parse_number(
                    "--uart-base",
                    &next_value(&mut args, "--uart-base")?,
                )?)
            }
            "--entry" => entry = Some(parse_number("--entry", &next_value(&mut args, "--entry")?)?),
            "--max-steps" => {
                let value = next_value(&mut args, "--max-steps")?;
                max_steps = if value == "unlimited" {
                    None
                } else {
                    Some(parse_number("--max-steps", &value)?)
                };
            }
            "--debug" => {
                debug = match next_value(&mut args, "--debug")?.as_str() {
                    "off" => DebugLevel::Off,
                    "pc" => DebugLevel::Pc,
                    "full" => DebugLevel::Full,
                    value => return Err(format!("invalid debug level: {value}")),
                }
            }
            "--" => {
                // `--` 后只接受唯一镜像路径，使以 `-` 开头的文件名不会被当作选项。
                let value = args
                    .next()
                    .ok_or_else(|| "missing image path after --".to_string())?;
                set_image(&mut image, value)?;
                if args.next().is_some() {
                    return Err("only one image path may be provided".into());
                }
                break;
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => set_image(&mut image, value.to_string())?,
        }
    }

    let image = image.ok_or_else(|| "missing image path".to_string())?;
    if platform == PlatformPreset::Bare && uart_base.is_some() {
        return Err("--uart-base conflicts with --platform bare".into());
    }
    Ok(Command::Run(CliOptions {
        image,
        format,
        platform,
        dram_base,
        dram_size,
        uart_base: uart_base.unwrap_or(cfg::UART_BASE),
        entry,
        max_steps,
        debug,
    }))
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("missing value for {option}"))
}

fn set_image(image: &mut Option<PathBuf>, value: String) -> Result<(), String> {
    if image.is_some() {
        return Err("only one image path may be provided".into());
    }
    *image = Some(PathBuf::from(value));
    Ok(())
}

fn parse_number(option: &str, value: &str) -> Result<u64, String> {
    // 先移除仅用于可读性的下划线，再根据前缀选择十进制或十六进制。
    let normalized = value.replace('_', "");
    let (digits, radix) = normalized
        .strip_prefix("0x")
        .map(|digits| (digits, 16))
        .unwrap_or((&normalized, 10));
    u64::from_str_radix(digits, radix).map_err(|_| format!("invalid value for {option}: {value}"))
}

fn parse_size(option: &str, value: &str) -> Result<usize, String> {
    let normalized = value.replace('_', "");
    let (digits, multiplier) = [
        ("GiB", 1024u64.pow(3)),
        ("MiB", 1024u64.pow(2)),
        ("KiB", 1024u64),
        ("G", 1024u64.pow(3)),
        ("M", 1024u64.pow(2)),
        ("K", 1024u64),
    ]
    .into_iter()
    .find_map(|(suffix, multiplier)| {
        normalized
            .strip_suffix(suffix)
            .map(|digits| (digits, multiplier))
    })
    .unwrap_or((&normalized, 1));
    let base =
        parse_number(option, digits).map_err(|_| format!("invalid size for {option}: {value}"))?;
    let bytes = base
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size for {option} overflows u64: {value}"))?;
    usize::try_from(bytes).map_err(|_| format!("size for {option} does not fit usize: {value}"))
}

fn usage() -> &'static str {
    r#"Usage: arvsim [OPTIONS] <IMAGE>

Options:
  --format <auto|flat|elf>   Image format [default: auto]
  --platform <bare|uart>     Attach DRAM only or DRAM plus UART [default: uart]
  --dram-base <ADDR>         DRAM base address [default: 0x80000000]
  --dram-size <SIZE>         DRAM size; supports K/M/G and KiB/MiB/GiB [default: 128MiB]
  --uart-base <ADDR>         UART base address [default: 0x10000000]
  --entry <ADDR>             Override image entry point
  --max-steps <N|unlimited>  Execution limit [default: 1000000]
  --debug <off|pc|full>      Execution trace detail [default: off]
  -h, --help                 Print this help"#
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|value| value.to_string()))
    }

    #[test]
    fn parses_image_and_defaults() {
        let Command::Run(options) = parse(&["guest.bin"]).unwrap() else {
            panic!("expected run command");
        };

        assert_eq!(options.image, PathBuf::from("guest.bin"));
        assert_eq!(options.format, ImageFormat::Auto);
        assert_eq!(options.max_steps, Some(DEFAULT_MAX_STEPS));
        assert_eq!(options.platform, PlatformPreset::Uart);
    }

    #[test]
    fn parses_runtime_and_platform_options() {
        let Command::Run(options) = parse(&[
            "--format",
            "elf",
            "--platform",
            "bare",
            "--dram-size",
            "64MiB",
            "--entry",
            "0x8000_1000",
            "--max-steps",
            "unlimited",
            "--debug",
            "pc",
            "kernel.elf",
        ])
        .unwrap() else {
            panic!("expected run command");
        };

        assert_eq!(options.format, ImageFormat::Elf);
        assert_eq!(options.platform, PlatformPreset::Bare);
        assert_eq!(options.dram_size, 64 * 1024 * 1024);
        assert_eq!(options.entry, Some(0x8000_1000));
        assert_eq!(options.max_steps, None);
        assert_eq!(options.debug, DebugLevel::Pc);
    }

    #[test]
    fn rejects_missing_and_duplicate_images() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["one.bin", "two.bin"]).is_err());
    }

    #[test]
    fn rejects_conflicting_and_identifies_invalid_numeric_options() {
        assert_eq!(
            parse(&[
                "--platform",
                "bare",
                "--uart-base",
                "0x10000000",
                "guest.bin",
            ])
            .unwrap_err(),
            "--uart-base conflicts with --platform bare"
        );
        assert_eq!(
            parse(&["--dram-base", "invalid", "guest.bin"]).unwrap_err(),
            "invalid value for --dram-base: invalid"
        );
        assert_eq!(
            parse(&["--dram-size", "128mib", "guest.bin"]).unwrap_err(),
            "invalid size for --dram-size: 128mib"
        );
    }
}
