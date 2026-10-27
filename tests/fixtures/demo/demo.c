#include "demo.h"

#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>

DEMO_API const int demo_const_global = 1234;
DEMO_API int demo_counter = 0;
DEMO_API int demo_freed = 0;
DEMO_API const char *const demo_names[] = {"alpha", "beta", NULL};

struct demo_handle { int value; };

int demo_add(int a, int b) { demo_counter++; return a + b; }

demo_point demo_point_add(demo_point a, demo_point b) {
    demo_point p = {a.x + b.x, a.y + b.y};
    return p;
}

demo_rect demo_rect_make(int32_t x, int32_t y, int32_t w, int32_t h) {
    demo_rect r = {{x, y}, {w, h}};
    return r;
}

int demo_area(const demo_rect *r) { return r->size.x * r->size.y; }

demo_handle *demo_handle_new(int value) {
    demo_handle *h = malloc(sizeof *h);
    h->value = value;
    return h;
}

int demo_handle_get(const demo_handle *h) { return h->value; }
void demo_handle_set(demo_handle *h, int value) { h->value = value; }
void demo_handle_free(demo_handle *h) { demo_freed++; free(h); }

int demo_call(demo_callback cb, void *user, int value) { return cb(user, value) * 2; }

int demo_sum(int count, ...) {
    va_list ap;
    int total = 0;
    va_start(ap, count);
    for (int i = 0; i < count; i++) total += va_arg(ap, int);
    va_end(ap);
    return total;
}

int demo_array_sum(const int values[4]) { return values[0] + values[1] + values[2] + values[3]; }

enum demo_color demo_mix(enum demo_color a, enum demo_color b) { return (enum demo_color)(a | b); }

demo_size demo_size_of(int n) { return n > 10 ? DEMO_LARGE : DEMO_SMALL; }

bool demo_is_even(int n) { return n % 2 == 0; }

struct demo_bits demo_bits_make(void) {
    struct demo_bits b = {0};
    b.a = 5;
    b.b = -3;
    b.flag = 1;
    b.color = DEMO_BLUE;
    b.c = 0xABCDE;
    b.tail = 0x7F;
    return b;
}

int demo_bits_check(struct demo_bits b) {
    return b.a == 6 && b.b == -16 && b.flag == 0 && b.color == DEMO_GREEN && b.c == 0x12345 &&
           b.tail == 9;
}

size_t demo_layout(int which) {
    switch (which) {
    case 0: return sizeof(struct demo_bits);
    case 1: return sizeof(struct demo_packed);
    case 2: return sizeof(struct demo_aligned);
    case 3: return sizeof(struct demo_pack2);
    case 4: return sizeof(union demo_number);
    case 5: return sizeof(struct demo_anon);
    case 6: return offsetof(struct demo_anon, pair);
    default: return 0;
    }
}

int demo_anon_sum(const struct demo_anon *a) { return a->kind + a->as_int + a->pair.lo + a->pair.hi; }

int demo_type(int type) { return type * 3; }

static int doubler(void *user, int value) { (void)user; return value * 2; }
demo_callback demo_get_callback(void) { return doubler; }

long double demo_long_double(void) { return 1.0L; }

int demo_check(const char *version, size_t size, int flags) {
    return strcmp(version, DEMO_VERSION) == 0 && size == sizeof(demo_rect) && flags == 7;
}

size_t demo_strlen(const char *s) { return strlen(s); }
unsigned demo_sum_bytes(const unsigned char *data, size_t len) {
    unsigned sum = 0;
    for (size_t i = 0; i < len; i++) sum += data[i];
    return sum;
}
int demo_fill(void *buf, unsigned len, int value) {
    memset(buf, value, len);
    return (int)len;
}
int demo_parse(const char *text, int *out) {
    char *end;
    long v = strtol(text, &end, 10);
    if (end == text || *end) return -1;
    *out = (int)v;
    return 0;
}
int demo_checked(int x) { return x < 0 ? -22 : x * 2; }
int demo_fail_errno(int code) {
    errno = code;
    return -1;
}
const char *demo_version(void) { return "demo 1.0"; }
int demo_handle_label(const demo_handle *h, char *buf, size_t len) {
    return snprintf(buf, len, "handle %d", h->value);
}
