#!/usr/bin/env node
/*
 * 1-million-row challenge -- Node.js (ESM) implementation.
 *
 * Usage: weather_js /absolute/path/to/measurements.txt
 *
 * Byte-for-byte equivalent of weather_c.c: aggregates station temperatures in
 * integer tenths of a degree using BigInt (64-bit-safe) and prints
 * {Station=min/mean/max, ...} sorted by station in UTF-8 byte order.
 * Standard library only -- no dependencies, no build step.
 */

import fs from 'node:fs';

const INT64_MAX = 9223372036854775807n;

function fail(message) {
  process.stderr.write(`weather_js: ${message}\n`);
  process.exit(1);
}

function failLine(lineNo, message) {
  process.stderr.write(`weather_js: malformed input at line ${lineNo}: ${message}\n`);
  process.exit(1);
}

function isBlank(byte) {
  return byte === 0x20 || byte === 0x09 || byte === 0x0d || byte === 0x0b || byte === 0x0c;
}

/* Mirrors the C parse_tenths(): optional '-', digits, '.', exactly one digit.
 * Overflow checks match the C magnitude checks, including for negatives. */
function parseTenths(bytes, lineNo) {
  let i = 0;
  const len = bytes.length;
  let negative = false;
  if (i < len && bytes[i] === 0x2d /* '-' */) {
    negative = true;
    i++;
  }
  if (i >= len) failLine(lineNo, 'empty temperature');

  let whole = 0n;
  let wholeDigits = 0;
  while (i < len && bytes[i] >= 0x30 && bytes[i] <= 0x39) {
    const digit = BigInt(bytes[i] - 0x30);
    if (whole > (INT64_MAX - digit) / 10n) failLine(lineNo, 'temperature out of range');
    whole = whole * 10n + digit;
    i++;
    wholeDigits++;
  }
  if (wholeDigits === 0 || i >= len || bytes[i] !== 0x2e /* '.' */) {
    failLine(lineNo, 'expected a decimal point and an integer part');
  }
  i++;
  if (i >= len || bytes[i] < 0x30 || bytes[i] > 0x39) {
    failLine(lineNo, 'expected exactly one fractional digit');
  }
  const fraction = BigInt(bytes[i] - 0x30);
  i++;
  if (i !== len) failLine(lineNo, 'expected exactly one fractional digit');
  if (whole > (INT64_MAX - fraction) / 10n) failLine(lineNo, 'temperature out of range');

  const tenths = whole * 10n + fraction;
  return negative ? -tenths : tenths;
}

function roundHalfAway(sum, count) {
  const magnitude = sum < 0n ? -sum : sum;
  let q = magnitude / count;
  const r = magnitude % count;
  if (r * 2n >= count) q++;
  return sum < 0n ? -q : q;
}

function formatTenths(value) {
  if (value === 0n) return '0.0';
  const magnitude = value < 0n ? -value : value;
  const sign = value < 0n ? '-' : '';
  return `${sign}${magnitude / 10n}.${magnitude % 10n}`;
}

function main() {
  if (process.argv.length !== 3) {
    process.stderr.write(`usage: ${process.argv[1]} /absolute/path/to/measurements.txt\n`);
    process.exit(2);
  }

  const path = process.argv[2];
  let data;
  try {
    data = fs.readFileSync(path);
  } catch (err) {
    fail(`cannot open '${path}': ${err.message}`);
  }

  /* latin1 is a bijection over bytes, so it is a safe Map key. The original
   * raw Buffer is retained as the station name for byte-order sorting/output. */
  const stations = new Map();
  let lineNo = 0;
  let start = 0;
  const total = data.length;

  while (start < total) {
    let newline = data.indexOf(0x0a /* '\n' */, start);
    if (newline === -1) newline = total;
    let end = newline;
    lineNo++;

    /* Strip trailing CR bytes, then ignore empty / whitespace-only lines. */
    while (end > start && data[end - 1] === 0x0d) end--;
    if (end > start) {
      let blank = true;
      for (let i = start; i < end; i++) {
        if (!isBlank(data[i])) {
          blank = false;
          break;
        }
      }

      if (!blank) {
        const line = data.subarray(start, end);
        const sep = line.indexOf(0x3b /* ';' */);
        if (sep === -1) failLine(lineNo, "missing ';' separator");
        if (sep === 0) failLine(lineNo, 'empty station name');

        const name = line.subarray(0, sep);
        const temp = line.subarray(sep + 1);
        if (temp.indexOf(0x3b) !== -1) failLine(lineNo, "more than one ';' separator");

        const tenths = parseTenths(temp, lineNo);
        const key = name.toString('latin1');
        let station = stations.get(key);
        if (station === undefined) {
          station = { name, min: tenths, max: tenths, sum: 0n, count: 0n };
          stations.set(key, station);
        } else {
          if (tenths < station.min) station.min = tenths;
          if (tenths > station.max) station.max = tenths;
        }
        station.sum += tenths;
        station.count += 1n;
      }
    }

    if (newline === total) break;
    start = newline + 1;
  }

  const keys = [...stations.keys()].sort((a, b) =>
    Buffer.compare(stations.get(a).name, stations.get(b).name)
  );

  const chunks = [Buffer.from('{')];
  for (let i = 0; i < keys.length; i++) {
    if (i > 0) chunks.push(Buffer.from(', '));
    const s = stations.get(keys[i]);
    chunks.push(s.name);
    chunks.push(Buffer.from('='));
    chunks.push(Buffer.from(formatTenths(s.min)));
    chunks.push(Buffer.from('/'));
    chunks.push(Buffer.from(formatTenths(roundHalfAway(s.sum, s.count))));
    chunks.push(Buffer.from('/'));
    chunks.push(Buffer.from(formatTenths(s.max)));
  }
  chunks.push(Buffer.from('}\n'));
  process.stdout.write(Buffer.concat(chunks));
}

main();
