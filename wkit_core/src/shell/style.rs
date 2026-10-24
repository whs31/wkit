pub use anstyle::Style;

pub const HEADER: anstyle::Style = anstyle::AnsiColor::Green.on_default().bold();
pub const PROGRESS: anstyle::Style = anstyle::AnsiColor::Cyan.on_default().bold();
pub const ERROR: anstyle::Style = anstyle::AnsiColor::Red.on_default().bold();
pub const WARN: anstyle::Style = anstyle::AnsiColor::Yellow.on_default().bold();
pub const NOTE: anstyle::Style = anstyle::AnsiColor::Cyan.on_default().bold();
pub const DIM: anstyle::Style = anstyle::Style::new().dimmed();
