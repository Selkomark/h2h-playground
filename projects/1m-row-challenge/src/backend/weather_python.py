#!/usr/bin/env python3
"""
1-million-row challenge -- Python 3 implementation (standard library only).

Usage: weather_python /absolute/path/to/measurements.txt

Byte-for-byte equivalent of weather_c.c: aggregates station temperatures in
integer tenths of a degree with signed-64-bit-style sums and prints
{Station=min/mean/max, ...} sorted by station in UTF-8 byte order.
"""

from __future__ import annotations

import sys


INT64_MAX = (1 << 63) - 1
INT64_MIN = -(1 << 63)
BLANK_BYTES = b" \t\r\v\f"

_MINUS = 0x2D  # b"-"
_DOT = 0x2E  # b"."
_ZERO = 0x30  # b"0"
_NINE = 0x39  # b"9"


def fail(message: str) -> None:
    print(f"weather_python: {message}", file=sys.stderr)
    raise SystemExit(1)


def fail_line(line_no: int, message: str) -> None:
    print(f"weather_python: malformed input at line {line_no}: {message}", file=sys.stderr)
    raise SystemExit(1)


def add64(value: int, increment: int) -> int:
    """Add like a signed 64-bit integer (two's-complement wraparound)."""
    result = value + increment
    if result > INT64_MAX:
        return result - (1 << 64)
    if result < INT64_MIN:
        return result + (1 << 64)
    return result


def parse_tenths(text: bytes, line_no: int) -> int:
    """Parse "-23.0" as -230, rejecting anything but exactly one fractional digit."""
    length = len(text)
    if length == 0:
        fail_line(line_no, "empty temperature")

    index = 0
    negative = text[0] == _MINUS
    if negative:
        index = 1
        if index == length:
            fail_line(line_no, "empty temperature")

    whole_start = index
    whole = 0
    while index < length and _ZERO <= text[index] <= _NINE:
        digit = text[index] - _ZERO
        if whole > (INT64_MAX - digit) // 10:
            fail_line(line_no, "temperature out of range")
        whole = whole * 10 + digit
        index += 1
    dot = index
    if dot == whole_start or dot == length or text[dot] != _DOT:
        fail_line(line_no, "expected a decimal point and an integer part")

    if dot + 1 >= length or not _ZERO <= text[dot + 1] <= _NINE:
        fail_line(line_no, "expected exactly one fractional digit")
    fraction = text[dot + 1] - _ZERO
    if dot + 2 != length:
        fail_line(line_no, "expected exactly one fractional digit")

    if whole > (INT64_MAX - fraction) // 10:
        fail_line(line_no, "temperature out of range")

    tenths = whole * 10 + fraction
    return -tenths if negative else tenths


def round_half_away(sum_t: int, count: int) -> int:
    """Round sum_t / count to the nearest tenth, ties away from zero."""
    magnitude = -sum_t if sum_t < 0 else sum_t
    quotient, remainder = divmod(magnitude, count)
    if remainder * 2 >= count:
        quotient += 1
    return -quotient if sum_t < 0 else quotient


def format_tenths(tenths: int) -> bytes:
    """Format an integer number of tenths; -0.0 is printed as 0.0."""
    if tenths == 0:
        return b"0.0"
    sign = b"-" if tenths < 0 else b""
    magnitude = -tenths if tenths < 0 else tenths
    return sign + str(magnitude // 10).encode("ascii") + b"." + bytes((_ZERO + magnitude % 10,))


def aggregate(path: str) -> dict[bytes, list[int]]:
    """Return {station name (raw bytes): [min, max, sum, count]}."""
    stations: dict[bytes, list[int]] = {}
    try:
        handle = open(path, "rb")
    except OSError as error:
        fail(f"cannot open '{path}': {error.strerror or error}")

    with handle:
        try:
            for line_no, raw in enumerate(handle, 1):
                if raw.endswith(b"\n"):
                    raw = raw[:-1]
                line = raw.rstrip(b"\r")

                # C ignores empty and whitespace-only lines.
                if not line.strip(BLANK_BYTES):
                    continue

                separator = line.find(b";")
                if separator < 0:
                    fail_line(line_no, "missing ';' separator")
                if separator == 0:
                    fail_line(line_no, "empty station name")
                temperature = line[separator + 1 :]
                if b";" in temperature:
                    fail_line(line_no, "more than one ';' separator")

                tenths = parse_tenths(temperature, line_no)
                name = line[:separator]
                stats = stations.get(name)
                if stats is None:
                    stations[name] = [tenths, tenths, tenths, 1]
                else:
                    if tenths < stats[0]:
                        stats[0] = tenths
                    if tenths > stats[1]:
                        stats[1] = tenths
                    stats[2] = add64(stats[2], tenths)
                    stats[3] += 1
        except OSError as error:
            fail(f"error reading '{path}': {error.strerror or error}")

    return stations


def main(argv: list[str] | None = None) -> int:
    arguments = sys.argv if argv is None else argv
    if len(arguments) != 2:
        print(
            f"usage: {arguments[0]} /absolute/path/to/measurements.txt",
            file=sys.stderr,
        )
        return 2

    stations = aggregate(arguments[1])

    output = bytearray(b"{")
    # bytes sort in UTF-8 byte order, matching strcmp / Rust String::cmp.
    for index, name in enumerate(sorted(stations)):
        if index:
            output.extend(b", ")
        min_t, max_t, sum_t, count = stations[name]
        output.extend(name)
        output.extend(b"=")
        output.extend(format_tenths(min_t))
        output.extend(b"/")
        output.extend(format_tenths(round_half_away(sum_t, count)))
        output.extend(b"/")
        output.extend(format_tenths(max_t))
    output.extend(b"}\n")
    sys.stdout.buffer.write(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
