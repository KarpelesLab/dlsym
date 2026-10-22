// A C++ library exercising what dlsym-gen binds from C++ headers.
#pragma once

#include <stdint.h>

#ifdef _WIN32
#define CPP_API __declspec(dllexport)
#else
#define CPP_API __attribute__((visibility("default")))
#endif

namespace geo {

/** A trivially copyable struct: passed by value. */
struct point {
    int32_t x, y;
};

enum class shape { circle = 1, square = 4 };
enum unit { mm, cm };

/** Overloads get a suffix from their parameter types. */
CPP_API int add(int a, int b) noexcept;
CPP_API double add(double a, double b) noexcept;
/** References are passed as pointers. */
CPP_API point translate(point p, const point &by) noexcept;
CPP_API int sides(shape s) noexcept;
/** May throw, so it is bound as `extern "C-unwind"`. */
CPP_API int checked_div(int a, int b);
CPP_API extern int counter;

class CPP_API Registry {
public:
    Registry();
    static int count() noexcept;
    static int total;
    int value() const;

private:
    static int secret() noexcept;
    int v;
};

/** A class with constructors, methods and a destructor: Rust gets an RAII
 * wrapper that allocates, constructs and destroys it. */
class CPP_API Counter {
public:
    Counter();
    explicit Counter(int start);
    Counter(const Counter &) = delete;
    ~Counter();
    int get() const noexcept;
    void add(int n);
    int add(int a, int b);
    static int live() noexcept;

private:
    int *value;
};

/** Not trivially copyable: only usable behind pointers. */
class Name {
public:
    ~Name();
    const char *s;
};
CPP_API int name_len(const Name &n) noexcept;
CPP_API Name make_name();

namespace detail {
CPP_API int version() noexcept;
}

}  // namespace geo

extern "C" CPP_API int cpp_plain(int x);
