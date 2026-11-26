// 1-million-row challenge -- Go implementation (single file, stdlib only).
//
// Usage: weather_go /absolute/path/to/measurements.txt
//
// Aggregates station temperatures in integer tenths of a degree and prints
// {Station=min/mean/max, ...} sorted by station in UTF-8 byte order.

package main

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"os"
	"sort"
	"strconv"
)

type stats struct {
	minT  int64
	maxT  int64
	sumT  int64
	count uint64
}

func failLine(lineNo int, message string) {
	fmt.Fprintf(os.Stderr, "weather_go: malformed input at line %d: %s\n", lineNo, message)
	os.Exit(1)
}

func isBlank(b byte) bool {
	return b == ' ' || b == '\t' || b == '\r' || b == '\v' || b == '\f'
}

// "-23.0" -> -230, rejecting anything without exactly one fractional digit.
func parseTenths(text []byte, lineNo int) int64 {
	i := 0
	negative := false
	if len(text) > 0 && text[0] == '-' {
		negative = true
		i++
	}
	if i >= len(text) {
		failLine(lineNo, "empty temperature")
	}

	var whole int64
	wholeDigits := 0
	for i < len(text) && text[i] >= '0' && text[i] <= '9' {
		digit := int64(text[i] - '0')
		if whole > (int64(^uint64(0)>>1)-digit)/10 {
			failLine(lineNo, "temperature out of range")
		}
		whole = whole*10 + digit
		i++
		wholeDigits++
	}
	if wholeDigits == 0 || i >= len(text) || text[i] != '.' {
		failLine(lineNo, "expected a decimal point and an integer part")
	}
	i++
	if i >= len(text) || text[i] < '0' || text[i] > '9' {
		failLine(lineNo, "expected exactly one fractional digit")
	}
	fraction := int64(text[i] - '0')
	i++
	if i != len(text) {
		failLine(lineNo, "expected exactly one fractional digit")
	}
	if whole > (int64(^uint64(0)>>1)-fraction)/10 {
		failLine(lineNo, "temperature out of range")
	}
	tenths := whole*10 + fraction
	if negative {
		return -tenths
	}
	return tenths
}

func roundHalfAway(sum int64, count uint64) int64 {
	magnitude := uint64(sum)
	if sum < 0 {
		magnitude = uint64(-(sum + 1)) + 1
	}
	quotient := magnitude / count
	if (magnitude%count)*2 >= count {
		quotient++
	}
	if sum < 0 {
		return -int64(quotient)
	}
	return int64(quotient)
}

func appendTenths(dst []byte, tenths int64) []byte {
	if tenths == 0 {
		return append(dst, '0', '.', '0')
	}
	magnitude := uint64(tenths)
	if tenths < 0 {
		dst = append(dst, '-')
		magnitude = uint64(-(tenths + 1)) + 1
	}
	dst = strconv.AppendUint(dst, magnitude/10, 10)
	dst = append(dst, '.', byte('0')+byte(magnitude%10))
	return dst
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintf(os.Stderr, "usage: %s /absolute/path/to/measurements.txt\n", os.Args[0])
		os.Exit(2)
	}

	file, err := os.Open(os.Args[1])
	if err != nil {
		fmt.Fprintf(os.Stderr, "weather_go: cannot open '%s': %v\n", os.Args[1], err)
		os.Exit(1)
	}
	defer file.Close()

	stations := make(map[string]*stats, 512)
	reader := bufio.NewReaderSize(file, 1<<20)
	lineNo := 0
	for {
		line, readErr := reader.ReadString('\n')
		if len(line) > 0 {
			lineNo++
			body := []byte(line)
			for len(body) > 0 && (body[len(body)-1] == '\n' || body[len(body)-1] == '\r') {
				body = body[:len(body)-1]
			}
			if len(body) == 0 {
				// blank line
			} else {
				blank := true
				for _, b := range body {
					if !isBlank(b) {
						blank = false
						break
					}
				}
				if !blank {
					separator := bytes.IndexByte(body, ';')
					if separator < 0 {
						failLine(lineNo, "missing ';' separator")
					}
					if separator == 0 {
						failLine(lineNo, "empty station name")
					}
					if bytes.IndexByte(body[separator+1:], ';') >= 0 {
						failLine(lineNo, "more than one ';' separator")
					}
					tenths := parseTenths(body[separator+1:], lineNo)
					name := string(body[:separator])
					entry := stations[name]
					if entry == nil {
						stations[name] = &stats{minT: tenths, maxT: tenths, sumT: tenths, count: 1}
					} else {
						if tenths < entry.minT {
							entry.minT = tenths
						}
						if tenths > entry.maxT {
							entry.maxT = tenths
						}
						entry.sumT += tenths
						entry.count++
					}
				}
			}
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			fmt.Fprintf(os.Stderr, "weather_go: error reading '%s': %v\n", os.Args[1], readErr)
			os.Exit(1)
		}
	}

	names := make([]string, 0, len(stations))
	for name := range stations {
		names = append(names, name)
	}
	sort.Strings(names) // byte order, matching strcmp / std::string / Rust String::cmp

	output := make([]byte, 0, len(names)*48+2)
	output = append(output, '{')
	for i, name := range names {
		if i > 0 {
			output = append(output, ',', ' ')
		}
		output = append(output, name...)
		output = append(output, '=')
		entry := stations[name]
		output = appendTenths(output, entry.minT)
		output = append(output, '/')
		output = appendTenths(output, roundHalfAway(entry.sumT, entry.count))
		output = append(output, '/')
		output = appendTenths(output, entry.maxT)
	}
	output = append(output, '}', '\n')

	writer := bufio.NewWriterSize(os.Stdout, 1<<20)
	if _, err := writer.Write(output); err != nil {
		fmt.Fprintf(os.Stderr, "weather_go: error writing output: %v\n", err)
		os.Exit(1)
	}
	if err := writer.Flush(); err != nil {
		fmt.Fprintf(os.Stderr, "weather_go: error writing output: %v\n", err)
		os.Exit(1)
	}
}
