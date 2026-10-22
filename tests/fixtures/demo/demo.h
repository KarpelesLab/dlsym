/* A C library exercising the cases dlsym-gen must handle. */
#ifndef DEMO_H
#define DEMO_H

#include <stdbool.h>
#include <stddef.h>
#include "demo_types.h"

#ifdef _WIN32
#define DEMO_API __declspec(dllexport)
#else
#define DEMO_API
#endif

#define DEMO_VERSION "1.2.3"
#define DEMO_ESCAPED "tab\there \"quoted\""
#define DEMO_ANSWER 42
#define DEMO_NEGATIVE (-7)
#define DEMO_FLAG (1u << 31)
#define DEMO_BIG 0x123456789ULL
#define DEMO_RATIO 0.25
#define DEMO_RATIO_F 1.5f
#define DEMO_CHAR 'A'
#define DEMO_DERIVED (DEMO_ANSWER * 2 + 1)
#define DEMO_EMPTY
#define DEMO_FUNC_LIKE(x) ((x) + 1)

/** Colors. */
enum demo_color { DEMO_RED = 1, DEMO_GREEN = 2, DEMO_BLUE = 4 };

typedef enum { DEMO_SMALL = -1, DEMO_LARGE = 1000 } demo_size;

enum { DEMO_ANON_A = 10, DEMO_ANON_B };

/** A rectangle with nested structs. */
typedef struct demo_rect {
    demo_point origin;
    demo_point size;
} demo_rect;

/** An opaque handle. */
typedef struct demo_handle demo_handle;

typedef int (*demo_callback)(void *user, int value);

struct demo_bits {
    unsigned int a : 3;
    signed int b : 5;
    bool flag : 1;
    enum demo_color color : 4;
    unsigned int : 0;
    unsigned int c : 20;
    uint8_t tail;
};

struct demo_packed {
    char c;
    int i;
    short s;
} __attribute__((packed));

struct demo_aligned {
    int value;
} __attribute__((aligned(16)));

#pragma pack(push, 2)
struct demo_pack2 {
    char c;
    int i;
    double d;
};
#pragma pack(pop)

union demo_number {
    int i;
    float f;
    unsigned char bytes[8];
};

struct demo_anon {
    int kind;
    union {
        int as_int;
        float as_float;
    };
    struct {
        short lo, hi;
    } pair;
};

struct demo_flex {
    size_t len;
    int data[];
};

struct demo_keywords {
    int type;
    int match;
    int self;
};

DEMO_API extern const int demo_const_global;
DEMO_API extern int demo_counter;
DEMO_API extern int demo_freed;
DEMO_API extern const char *const demo_names[];

DEMO_API int demo_add(int a, int b);
DEMO_API demo_point demo_point_add(demo_point a, demo_point b);
DEMO_API demo_rect demo_rect_make(int32_t x, int32_t y, int32_t w, int32_t h);
DEMO_API int demo_area(const demo_rect *r);
DEMO_API demo_handle *demo_handle_new(int value);
DEMO_API int demo_handle_get(const demo_handle *h);
DEMO_API void demo_handle_set(demo_handle *h, int value);
DEMO_API void demo_handle_free(demo_handle *h);
DEMO_API int demo_call(demo_callback cb, void *user, int value);
DEMO_API int demo_sum(int count, ...);
DEMO_API int demo_array_sum(const int values[4]);
DEMO_API enum demo_color demo_mix(enum demo_color a, enum demo_color b);
DEMO_API demo_size demo_size_of(int n);
DEMO_API bool demo_is_even(int n);
DEMO_API struct demo_bits demo_bits_make(void);
DEMO_API int demo_bits_check(struct demo_bits b);
DEMO_API size_t demo_layout(int which);
DEMO_API int demo_anon_sum(const struct demo_anon *a);
DEMO_API int demo_type(int type);
DEMO_API demo_callback demo_get_callback(void);
DEMO_API long double demo_long_double(void);
DEMO_API int demo_check(const char *version, size_t size, int flags);

/* Function-like macros wrapping a call become methods; others are ignored. */
#define demo_check_default(flags) demo_check(DEMO_VERSION, sizeof(demo_rect), (flags))
#define demo_add_ten(x) demo_add((int)(x), 10)
#define demo_twice(x) ((x) * 2)

/* Declared but not defined in the library: bound as #[optional]. */
DEMO_API int demo_not_implemented(void);

static inline int demo_inline(void) { return 1; }

#endif
