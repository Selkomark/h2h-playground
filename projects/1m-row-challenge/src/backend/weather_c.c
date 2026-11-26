/*
 * 1-million-row challenge -- C11 implementation.
 *
 * Usage: weather_c /absolute/path/to/measurements.txt
 *
 * Aggregates station temperatures in integer tenths of a degree and prints
 * {Station=min/mean/max, ...} sorted by station in UTF-8 byte order.
 */
#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define READ_BUFFER_SIZE (1u << 20)
#define INITIAL_SLOTS 1024u

typedef struct {
    char *name;          /* NUL-terminated copy of the station name */
    size_t name_len;
    int64_t min_t;       /* integer tenths of a degree */
    int64_t max_t;
    int64_t sum_t;
    uint64_t count;
} Station;

typedef struct {
    FILE *file;
    unsigned char buffer[READ_BUFFER_SIZE];
    size_t pos;
    size_t len;
} Reader;

static Station *stations = NULL;
static size_t station_count = 0;
static size_t station_cap = 0;
static size_t *slots = NULL;         /* index + 1, 0 means empty */
static size_t slot_count = 0;

static void fatal(const char *message)
{
    fprintf(stderr, "weather_c: %s\n", message);
    exit(1);
}

static void fatal_line(size_t line_no, const char *message)
{
    fprintf(stderr, "weather_c: malformed input at line %zu: %s\n", line_no, message);
    exit(1);
}

static uint64_t fnv1a(const char *data, size_t len)
{
    uint64_t hash = UINT64_C(1469598103934665603);
    for (size_t i = 0; i < len; i++) {
        hash ^= (uint64_t)(unsigned char)data[i];
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

static void *checked_realloc(void *ptr, size_t size)
{
    void *result = realloc(ptr, size);
    if (result == NULL) {
        fatal("out of memory");
    }
    return result;
}

static void allocate_slots(size_t wanted)
{
    size_t cap = INITIAL_SLOTS;
    while (cap < wanted * 2u) {
        cap *= 2u;
    }
    slots = checked_realloc(NULL, cap * sizeof(*slots));
    memset(slots, 0, cap * sizeof(*slots));
    slot_count = cap;
}

static void insert_slot(size_t index)
{
    size_t mask = slot_count - 1u;
    size_t pos = (size_t)(fnv1a(stations[index].name, stations[index].name_len) & mask);
    while (slots[pos] != 0) {
        pos = (pos + 1u) & mask;
    }
    slots[pos] = index + 1u;
}

static void grow_slots(void)
{
    size_t old_count = slot_count;
    size_t *old = slots;
    allocate_slots(station_count + 1u);
    for (size_t i = 0; i < old_count; i++) {
        if (old[i] != 0) {
            insert_slot(old[i] - 1u);
        }
    }
    free(old);
}

/* Returns the station index, creating the station on first sight. */
static size_t get_or_create_station(const char *name, size_t name_len)
{
    if (slots == NULL) {
        allocate_slots(INITIAL_SLOTS / 2u);
    }
    if ((station_count + 1u) * 10u >= slot_count * 7u) {
        grow_slots();
    }

    size_t mask = slot_count - 1u;
    size_t pos = (size_t)(fnv1a(name, name_len) & mask);
    while (slots[pos] != 0) {
        size_t index = slots[pos] - 1u;
        if (stations[index].name_len == name_len && memcmp(stations[index].name, name, name_len) == 0) {
            return index;
        }
        pos = (pos + 1u) & mask;
    }

    if (station_count == station_cap) {
        station_cap = station_cap == 0 ? 256u : station_cap * 2u;
        stations = checked_realloc(stations, station_cap * sizeof(*stations));
    }
    size_t index = station_count++;
    char *copy = checked_realloc(NULL, name_len + 1u);
    memcpy(copy, name, name_len);
    copy[name_len] = '\0';
    stations[index].name = copy;
    stations[index].name_len = name_len;
    stations[index].min_t = 0;
    stations[index].max_t = 0;
    stations[index].sum_t = 0;
    stations[index].count = 0;

    /* A concurrent growth may have changed the table; re-insert via helper. */
    insert_slot(index);
    return index;
}

/* Reads one line (without '\n') into a reusable, NUL-terminated buffer.
 * Returns 1 for a line, 0 at end of file. */
static int next_line(Reader *reader, char **line, size_t *cap)
{
    size_t used = 0;
    for (;;) {
        if (reader->pos == reader->len) {
            reader->len = fread(reader->buffer, 1, sizeof(reader->buffer), reader->file);
            reader->pos = 0;
            if (reader->len == 0) {
                if (used == 0) {
                    return 0;
                }
                break;
            }
        }
        unsigned char *newline = memchr(reader->buffer + reader->pos, '\n', reader->len - reader->pos);
        size_t take = newline != NULL
                          ? (size_t)(newline - (reader->buffer + reader->pos))
                          : reader->len - reader->pos;
        if (used + take + 1u > *cap) {
            size_t wanted = used + take + 1u;
            size_t new_cap = *cap == 0 ? 256u : *cap;
            while (new_cap < wanted) {
                new_cap *= 2u;
            }
            *line = checked_realloc(*line, new_cap);
            *cap = new_cap;
        }
        memcpy(*line + used, reader->buffer + reader->pos, take);
        used += take;
        reader->pos += take + (newline != NULL ? 1u : 0u);
        if (newline != NULL) {
            break;
        }
    }
    (*line)[used] = '\0';
    return 1;
}

static int is_blank(unsigned char c)
{
    return c == ' ' || c == '\t' || c == '\r' || c == '\v' || c == '\f';
}

/* Parse "-23.0" -> -230 exactly, rejecting anything with != 1 fractional digit. */
static int64_t parse_tenths(const char *text, size_t len, size_t line_no)
{
    size_t i = 0;
    int negative = 0;
    if (i < len && text[i] == '-') {
        negative = 1;
        i++;
    }
    if (i >= len) {
        fatal_line(line_no, "empty temperature");
    }

    int64_t whole = 0;
    size_t whole_digits = 0;
    while (i < len && text[i] >= '0' && text[i] <= '9') {
        int digit = text[i] - '0';
        if (whole > (INT64_MAX - digit) / 10) {
            fatal_line(line_no, "temperature out of range");
        }
        whole = whole * 10 + digit;
        i++;
        whole_digits++;
    }
    if (whole_digits == 0 || i >= len || text[i] != '.') {
        fatal_line(line_no, "expected a decimal point and an integer part");
    }
    i++;
    if (i >= len || text[i] < '0' || text[i] > '9') {
        fatal_line(line_no, "expected exactly one fractional digit");
    }
    int64_t fraction = text[i] - '0';
    i++;
    if (i != len) {
        fatal_line(line_no, "expected exactly one fractional digit");
    }
    if (whole > (INT64_MAX - fraction) / 10) {
        fatal_line(line_no, "temperature out of range");
    }
    int64_t tenths = whole * 10 + fraction;
    return negative ? -tenths : tenths;
}

static int64_t round_half_away(int64_t sum, uint64_t count)
{
    uint64_t magnitude = sum < 0 ? (uint64_t)(-(sum + 1)) + 1u : (uint64_t)sum;
    uint64_t quotient = magnitude / count;
    uint64_t remainder = magnitude % count;
    if (remainder * 2u >= count) {
        quotient++;
    }
    return sum < 0 ? -(int64_t)quotient : (int64_t)quotient;
}

static void print_tenths(int64_t tenths)
{
    if (tenths == 0) {
        fputs("0.0", stdout);
        return;
    }
    uint64_t magnitude = tenths < 0 ? (uint64_t)(-(tenths + 1)) + 1u : (uint64_t)tenths;
    if (tenths < 0) {
        fputc('-', stdout);
    }
    printf("%" PRIu64 ".%u", magnitude / 10u, (unsigned)(magnitude % 10u));
}

static int compare_indices(const void *left, const void *right)
{
    size_t a = *(const size_t *)left;
    size_t b = *(const size_t *)right;
    return strcmp(stations[a].name, stations[b].name);
}

int main(int argc, char **argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: %s /absolute/path/to/measurements.txt\n", argv[0]);
        return 2;
    }

    FILE *file = fopen(argv[1], "rb");
    if (file == NULL) {
        fprintf(stderr, "weather_c: cannot open '%s': %s\n", argv[1], strerror(errno));
        return 1;
    }

    Reader reader;
    memset(&reader, 0, sizeof(reader));
    reader.file = file;

    char *line = NULL;
    size_t line_cap = 0;
    size_t line_no = 0;

    while (next_line(&reader, &line, &line_cap)) {
        line_no++;
        size_t len = strlen(line);
        while (len > 0 && line[len - 1] == '\r') {
            len--;
        }
        if (len == 0) {
            continue;
        }
        int blank = 1;
        for (size_t i = 0; i < len; i++) {
            if (!is_blank((unsigned char)line[i])) {
                blank = 0;
                break;
            }
        }
        if (blank) {
            continue;
        }

        char *separator = memchr(line, ';', len);
        if (separator == NULL) {
            fatal_line(line_no, "missing ';' separator");
        }
        size_t name_len = (size_t)(separator - line);
        if (name_len == 0) {
            fatal_line(line_no, "empty station name");
        }
        size_t temp_len = len - name_len - 1u;
        if (memchr(separator + 1, ';', temp_len) != NULL) {
            fatal_line(line_no, "more than one ';' separator");
        }

        int64_t tenths = parse_tenths(separator + 1, temp_len, line_no);
        size_t index = get_or_create_station(line, name_len);
        Station *station = &stations[index];
        if (station->count == 0) {
            station->min_t = station->max_t = tenths;
        } else {
            if (tenths < station->min_t) {
                station->min_t = tenths;
            }
            if (tenths > station->max_t) {
                station->max_t = tenths;
            }
        }
        station->sum_t += tenths;
        station->count++;
    }

    free(line);
    if (ferror(file)) {
        fprintf(stderr, "weather_c: error reading '%s'\n", argv[1]);
        fclose(file);
        return 1;
    }
    fclose(file);

    size_t *order = checked_realloc(NULL, (station_count == 0 ? 1u : station_count) * sizeof(*order));
    for (size_t i = 0; i < station_count; i++) {
        order[i] = i;
    }
    qsort(order, station_count, sizeof(*order), compare_indices);

    fputc('{', stdout);
    for (size_t i = 0; i < station_count; i++) {
        const Station *station = &stations[order[i]];
        if (i > 0) {
            fputs(", ", stdout);
        }
        fputs(station->name, stdout);
        fputc('=', stdout);
        print_tenths(station->min_t);
        fputc('/', stdout);
        print_tenths(round_half_away(station->sum_t, station->count));
        fputc('/', stdout);
        print_tenths(station->max_t);
    }
    fputs("}\n", stdout);

    free(order);
    for (size_t i = 0; i < station_count; i++) {
        free(stations[i].name);
    }
    free(stations);
    free(slots);
    return 0;
}
