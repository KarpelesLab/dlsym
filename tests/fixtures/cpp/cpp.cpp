#include "cpp.hpp"

namespace geo {

int add(int a, int b) noexcept { return a + b; }
double add(double a, double b) noexcept { return a + b + 0.5; }
point translate(point p, const point &by) noexcept { return {p.x + by.x, p.y + by.y}; }
int sides(shape s) noexcept { return static_cast<int>(s); }
int checked_div(int a, int b) {
    if (b == 0) throw a;
    return a / b;
}
int counter = 3;

Registry::Registry() : v(1) {}
int Registry::count() noexcept { return 7; }
int Registry::total = 11;
int Registry::value() const { return v; }
int Registry::secret() noexcept { return 13; }

Name::~Name() {}
int name_len(const Name &n) noexcept {
    int len = 0;
    while (n.s[len]) len++;
    return len;
}
Name make_name() { return Name{"geo"}; }

namespace detail {
int version() noexcept { return 2; }
}

}  // namespace geo

int cpp_plain(int x) { return x * 2; }
