/* Types shared by demo.h, pulled in with a quoted include. */
#ifndef DEMO_TYPES_H
#define DEMO_TYPES_H

#include <stdint.h>

/** A 2D point. */
typedef struct demo_point {
    int32_t x; /**< Horizontal. */
    int32_t y; /**< Vertical. */
} demo_point;

#endif
