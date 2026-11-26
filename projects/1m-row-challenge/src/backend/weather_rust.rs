// 1-million-row challenge -- Rust implementation (rustc 1.76 compatible, no crates).
//
// Usage: weather_rust /absolute/path/to/measurements.txt
//
// Aggregates station temperatures in integer tenths of a degree and prints
// {Station=min/mean/max, ...} sorted by station in UTF-8 byte order.

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process;

#[derive(Clone, Copy)]
struct Stats {
    min_t: i64,
    max_t: i64,
    sum_t: i64,
    count: u64,
}

fn fail_line(line_no: usize, message: &str) -> ! {
    eprintln!("weather_rust: malformed input at line {}: {}", line_no, message);
    process::exit(1);
}

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | 0x0b | 0x0c)
}

// "-23.0" -> -230, rejecting anything without exactly one fractional digit.
fn parse_tenths(text: &[u8], line_no: usize) -> i64 {
    let mut i = 0usize;
    let negative = if text.first() == Some(&b'-') {
        i = 1;
        true
    } else {
        false
    };
    if i >= text.len() {
        fail_line(line_no, "empty temperature");
    }

    let mut whole: i64 = 0;
    let mut whole_digits = 0usize;
    while i < text.len() && text[i].is_ascii_digit() {
        let digit = (text[i] - b'0') as i64;
        if whole > (i64::MAX - digit) / 10 {
            fail_line(line_no, "temperature out of range");
        }
        whole = whole * 10 + digit;
        i += 1;
        whole_digits += 1;
    }
    if whole_digits == 0 || i >= text.len() || text[i] != b'.' {
        fail_line(line_no, "expected a decimal point and an integer part");
    }
    i += 1;
    if i >= text.len() || !text[i].is_ascii_digit() {
        fail_line(line_no, "expected exactly one fractional digit");
    }
    let fraction = (text[i] - b'0') as i64;
    i += 1;
    if i != text.len() {
        fail_line(line_no, "expected exactly one fractional digit");
    }
    if whole > (i64::MAX - fraction) / 10 {
        fail_line(line_no, "temperature out of range");
    }
    let tenths = whole * 10 + fraction;
    if negative {
        -tenths
    } else {
        tenths
    }
}

fn round_half_away(sum: i64, count: u64) -> i64 {
    let magnitude = sum.unsigned_abs();
    let mut quotient = magnitude / count;
    if (magnitude % count) * 2 >= count {
        quotient += 1;
    }
    if sum < 0 {
        -(quotient as i64)
    } else {
        quotient as i64
    }
}

fn write_tenths<W: Write>(out: &mut W, tenths: i64) -> io::Result<()> {
    if tenths == 0 {
        return out.write_all(b"0.0");
    }
    let magnitude = tenths.unsigned_abs();
    if tenths < 0 {
        out.write_all(b"-")?;
    }
    write!(out, "{}.{}", magnitude / 10, magnitude % 10)
}

fn run(path: &str) -> io::Result<()> {
    let file = File::open(path).unwrap_or_else(|error| {
        eprintln!("weather_rust: cannot open '{}': {}", path, error);
        process::exit(1);
    });
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut stations: HashMap<String, Stats> = HashMap::with_capacity(512);

    let mut line = String::new();
    let mut line_no = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        line_no += 1;
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        if line.is_empty() || line.bytes().all(is_blank) {
            continue;
        }

        let bytes = line.as_bytes();
        let separator = match bytes.iter().position(|&byte| byte == b';') {
            Some(position) => position,
            None => fail_line(line_no, "missing ';' separator"),
        };
        if separator == 0 {
            fail_line(line_no, "empty station name");
        }
        if bytes[separator + 1..].contains(&b';') {
            fail_line(line_no, "more than one ';' separator");
        }

        let tenths = parse_tenths(&bytes[separator + 1..], line_no);
        let name = &line[..separator]; // ';' is ASCII, so this is a UTF-8 boundary
        match stations.get_mut(name) {
            Some(stats) => {
                if tenths < stats.min_t {
                    stats.min_t = tenths;
                }
                if tenths > stats.max_t {
                    stats.max_t = tenths;
                }
                stats.sum_t += tenths;
                stats.count += 1;
            }
            None => {
                stations.insert(
                    name.to_owned(),
                    Stats {
                        min_t: tenths,
                        max_t: tenths,
                        sum_t: tenths,
                        count: 1,
                    },
                );
            }
        }
    }

    let mut names: Vec<&String> = stations.keys().collect();
    names.sort(); // String::cmp compares UTF-8 bytes, like strcmp / Go string ordering

    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(1 << 20, stdout.lock());
    out.write_all(b"{")?;
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            out.write_all(b", ")?;
        }
        let stats = &stations[*name];
        write!(out, "{}=", name)?;
        write_tenths(&mut out, stats.min_t)?;
        out.write_all(b"/")?;
        write_tenths(&mut out, round_half_away(stats.sum_t, stats.count))?;
        out.write_all(b"/")?;
        write_tenths(&mut out, stats.max_t)?;
    }
    out.write_all(b"}\n")?;
    out.flush()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} /absolute/path/to/measurements.txt", args[0]);
        process::exit(2);
    }
    if let Err(error) = run(&args[1]) {
        eprintln!("weather_rust: I/O error: {}", error);
        process::exit(1);
    }
}
