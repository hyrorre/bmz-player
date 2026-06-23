use std::fmt;
use std::io::{self, Write};

pub struct SafeStderr;

pub struct SafeStderrWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SafeStderr {
    type Writer = SafeStderrWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SafeStderrWriter
    }
}

impl Write for SafeStderrWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match io::stderr().write(buf) {
            Ok(written) => Ok(written),
            Err(_) => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        Ok(())
    }
}

pub fn stdout_line(args: fmt::Arguments<'_>) {
    write_line(io::stdout().lock(), args);
}

pub fn stderr_line(args: fmt::Arguments<'_>) {
    write_line(io::stderr().lock(), args);
}

fn write_line(mut writer: impl Write, args: fmt::Arguments<'_>) {
    let _ = writer.write_fmt(args);
    let _ = writer.write_all(b"\n");
}
