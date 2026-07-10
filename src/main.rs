use arvsim::cpu::{Cpu, DebugLevel, RunOptions, RunOutcome};
use arvsim::loader::{self, ImageFormat};
use arvsim::{bus, cfg, dram, uart};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_MAX_STEPS: u64 = 1_000_000;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Platform {
    Bare,
    Uart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    image: PathBuf,
    format: ImageFormat,
    platform: Platform,
    dram_base: u64,
    dram_size: usize,
    uart_base: u64,
    entry: Option<u64>,
    max_steps: Option<u64>,
    debug: DebugLevel,
}

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
    let dram_size_u64 = match u64::try_from(options.dram_size) {
        Ok(size) if size > 0 => size,
        _ => {
            eprintln!("error: DRAM size must be a non-zero value representable as u64");
            return ExitCode::from(2);
        }
    };
    let dram_end = match options.dram_base.checked_add(dram_size_u64) {
        Some(end) => end,
        None => {
            eprintln!("error: DRAM address range overflows u64");
            return ExitCode::from(2);
        }
    };
    if options.platform == Platform::Uart {
        let Some(uart_end) = options.uart_base.checked_add(0x100) else {
            eprintln!("error: UART address range overflows u64");
            return ExitCode::from(2);
        };
        if ranges_overlap(options.dram_base, dram_end, options.uart_base, uart_end) {
            eprintln!("error: UART and DRAM address ranges overlap");
            return ExitCode::from(2);
        }
    }

    let mut dram = dram::Dram::with_layout(options.dram_base, options.dram_size);
    let loaded = match loader::load_image(&mut dram, &options.image, options.format) {
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
    if !(options.dram_base..dram_end).contains(&entry) {
        eprintln!(
            "error: entry point {entry:#x} is outside DRAM range {:#x}..{dram_end:#x}",
            options.dram_base
        );
        return ExitCode::from(2);
    }

    let mut bus = bus::Bus::new();
    bus.attach_device(options.dram_base, dram_size_u64, Box::new(dram));
    if options.platform == Platform::Uart {
        let uart = uart::Uart::new(options.uart_base);
        bus.attach_device(options.uart_base, 0x100, Box::new(uart));
    }

    let mut cpu = Cpu::with_reset_vector(Box::new(bus), entry, dram_end);
    cpu.reset();
    match cpu.run(RunOptions {
        max_steps: options.max_steps,
        debug: options.debug,
    }) {
        RunOutcome::StepLimitReached { steps } => {
            println!(
                "stopped after {steps} steps at pc={:#x} (step limit reached)",
                cpu.pc
            );
            ExitCode::SUCCESS
        }
        RunOutcome::Exception {
            steps,
            pc,
            exception,
        } => {
            eprintln!("error: guest exception after {steps} steps at pc={pc:#x}: {exception:?}");
            ExitCode::from(1)
        }
    }
}

fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut format = ImageFormat::Auto;
    let mut platform = Platform::Uart;
    let mut dram_base = cfg::DRAM_BASE;
    let mut dram_size = cfg::DRAM_SIZE;
    let mut uart_base = cfg::UART_BASE;
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
                    "bare" => Platform::Bare,
                    "uart" => Platform::Uart,
                    value => return Err(format!("invalid platform: {value}")),
                }
            }
            "--dram-base" => dram_base = parse_number(&next_value(&mut args, "--dram-base")?)?,
            "--dram-size" => dram_size = parse_size(&next_value(&mut args, "--dram-size")?)?,
            "--uart-base" => uart_base = parse_number(&next_value(&mut args, "--uart-base")?)?,
            "--entry" => entry = Some(parse_number(&next_value(&mut args, "--entry")?)?),
            "--max-steps" => {
                let value = next_value(&mut args, "--max-steps")?;
                max_steps = if value == "unlimited" {
                    None
                } else {
                    Some(parse_number(&value)?)
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
    Ok(Command::Run(CliOptions {
        image,
        format,
        platform,
        dram_base,
        dram_size,
        uart_base,
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

fn parse_number(value: &str) -> Result<u64, String> {
    let normalized = value.replace('_', "");
    let (digits, radix) = normalized
        .strip_prefix("0x")
        .map(|digits| (digits, 16))
        .unwrap_or((&normalized, 10));
    u64::from_str_radix(digits, radix).map_err(|_| format!("invalid number: {value}"))
}

fn parse_size(value: &str) -> Result<usize, String> {
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
    let base = parse_number(digits)?;
    let bytes = base
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size overflows u64: {value}"))?;
    usize::try_from(bytes).map_err(|_| format!("size does not fit usize: {value}"))
}

fn ranges_overlap(lhs_start: u64, lhs_end: u64, rhs_start: u64, rhs_end: u64) -> bool {
    lhs_start < rhs_end && rhs_start < lhs_end
}

fn usage() -> &'static str {
    r#"Usage: arvsim [OPTIONS] <IMAGE>

Options:
  --format <auto|flat|elf>   Image format [default: auto]
  --platform <bare|uart>     Attach DRAM only or DRAM plus UART [default: uart]
  --dram-base <ADDR>         DRAM base address [default: 0x80000000]
  --dram-size <SIZE>         DRAM size; supports KiB/MiB/GiB [default: 128MiB]
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
        assert_eq!(options.platform, Platform::Uart);
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
        assert_eq!(options.platform, Platform::Bare);
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
    fn detects_overlapping_ranges() {
        assert!(ranges_overlap(0x1000, 0x2000, 0x1800, 0x2800));
        assert!(!ranges_overlap(0x1000, 0x2000, 0x2000, 0x2800));
    }
}
