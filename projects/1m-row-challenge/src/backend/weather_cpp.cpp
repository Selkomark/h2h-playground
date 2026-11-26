// 1-million-row challenge -- C++17 implementation.
//
// Usage: weather_cpp /absolute/path/to/measurements.txt
//
// Aggregates station temperatures in integer tenths of a degree and prints
// {Station=min/mean/max, ...} sorted by station in UTF-8 byte order.

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace {

struct Stats {
    std::int64_t min_t;
    std::int64_t max_t;
    std::int64_t sum_t;
    std::uint64_t count;
};

[[noreturn]] void fail_line(std::size_t line_no, const char* message)
{
    std::cerr << "weather_cpp: malformed input at line " << line_no << ": " << message << '\n';
    std::exit(1);
}

bool is_blank(unsigned char c)
{
    return c == ' ' || c == '\t' || c == '\r' || c == '\v' || c == '\f';
}

// "-23.0" -> -230, rejecting anything that does not have exactly one decimal digit.
std::int64_t parse_tenths(const char* text, std::size_t len, std::size_t line_no)
{
    std::size_t i = 0;
    bool negative = false;
    if (i < len && text[i] == '-') {
        negative = true;
        ++i;
    }
    if (i >= len) {
        fail_line(line_no, "empty temperature");
    }

    std::int64_t whole = 0;
    std::size_t whole_digits = 0;
    while (i < len && text[i] >= '0' && text[i] <= '9') {
        int digit = text[i] - '0';
        if (whole > (INT64_MAX - digit) / 10) {
            fail_line(line_no, "temperature out of range");
        }
        whole = whole * 10 + digit;
        ++i;
        ++whole_digits;
    }
    if (whole_digits == 0 || i >= len || text[i] != '.') {
        fail_line(line_no, "expected a decimal point and an integer part");
    }
    ++i;
    if (i >= len || text[i] < '0' || text[i] > '9') {
        fail_line(line_no, "expected exactly one fractional digit");
    }
    std::int64_t fraction = text[i] - '0';
    ++i;
    if (i != len) {
        fail_line(line_no, "expected exactly one fractional digit");
    }
    if (whole > (INT64_MAX - fraction) / 10) {
        fail_line(line_no, "temperature out of range");
    }
    std::int64_t tenths = whole * 10 + fraction;
    return negative ? -tenths : tenths;
}

std::int64_t round_half_away(std::int64_t sum, std::uint64_t count)
{
    std::uint64_t magnitude = sum < 0 ? static_cast<std::uint64_t>(-(sum + 1)) + 1u
                                      : static_cast<std::uint64_t>(sum);
    std::uint64_t quotient = magnitude / count;
    if ((magnitude % count) * 2u >= count) {
        ++quotient;
    }
    return sum < 0 ? -static_cast<std::int64_t>(quotient) : static_cast<std::int64_t>(quotient);
}

void append_tenths(std::string& out, std::int64_t tenths)
{
    if (tenths == 0) {
        out += "0.0";
        return;
    }
    std::uint64_t magnitude = tenths < 0 ? static_cast<std::uint64_t>(-(tenths + 1)) + 1u
                                         : static_cast<std::uint64_t>(tenths);
    if (tenths < 0) {
        out.push_back('-');
    }
    out += std::to_string(magnitude / 10u);
    out.push_back('.');
    out += std::to_string(magnitude % 10u);
}

}  // namespace

int main(int argc, char** argv)
{
    std::ios::sync_with_stdio(false);
    std::cin.tie(nullptr);

    if (argc != 2) {
        std::cerr << "usage: " << argv[0] << " /absolute/path/to/measurements.txt\n";
        return 2;
    }

    std::ifstream input(argv[1], std::ios::binary);
    if (!input) {
        std::cerr << "weather_cpp: cannot open '" << argv[1] << "'\n";
        return 1;
    }

    std::unordered_map<std::string, Stats> stations;
    stations.reserve(1024);

    std::string line;
    std::size_t line_no = 0;
    while (std::getline(input, line)) {
        ++line_no;
        while (!line.empty() && line.back() == '\r') {
            line.pop_back();
        }
        if (line.empty()) {
            continue;
        }
        bool blank = true;
        for (unsigned char c : line) {
            if (!is_blank(c)) {
                blank = false;
                break;
            }
        }
        if (blank) {
            continue;
        }

        std::size_t separator = line.find(';');
        if (separator == std::string::npos) {
            fail_line(line_no, "missing ';' separator");
        }
        if (separator == 0) {
            fail_line(line_no, "empty station name");
        }
        if (line.find(';', separator + 1) != std::string::npos) {
            fail_line(line_no, "more than one ';' separator");
        }

        std::string name = line.substr(0, separator);
        std::int64_t tenths = parse_tenths(line.data() + separator + 1, line.size() - separator - 1, line_no);

        auto found = stations.find(name);
        if (found == stations.end()) {
            stations.emplace(std::move(name), Stats{tenths, tenths, tenths, 1});
        } else {
            Stats& stats = found->second;
            if (tenths < stats.min_t) {
                stats.min_t = tenths;
            }
            if (tenths > stats.max_t) {
                stats.max_t = tenths;
            }
            stats.sum_t += tenths;
            ++stats.count;
        }
    }
    if (input.bad()) {
        std::cerr << "weather_cpp: error reading '" << argv[1] << "'\n";
        return 1;
    }

    std::vector<std::pair<std::string, Stats>> rows(stations.begin(), stations.end());
    std::sort(rows.begin(), rows.end(), [](const auto& a, const auto& b) { return a.first < b.first; });

    std::string output;
    output.reserve(rows.size() * 48u + 2u);
    output.push_back('{');
    for (std::size_t i = 0; i < rows.size(); ++i) {
        if (i > 0) {
            output += ", ";
        }
        const std::string& name = rows[i].first;
        const Stats& stats = rows[i].second;
        output += name;
        output.push_back('=');
        append_tenths(output, stats.min_t);
        output.push_back('/');
        append_tenths(output, round_half_away(stats.sum_t, stats.count));
        output.push_back('/');
        append_tenths(output, stats.max_t);
    }
    output += "}\n";
    std::cout.write(output.data(), static_cast<std::streamsize>(output.size()));
    return 0;
}
