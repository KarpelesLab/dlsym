//! Exercises bindings generated from `cpp.hpp` against the compiled library.
//! Built and run by `tests/codegen.rs`.

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    clippy::all
)]
mod bindings {
    include!("bindings.rs");
}

use bindings::*;
use dlsym::Bindings;

fn main() {
    let path = std::env::args().nth(1).expect("library path");
    // SAFETY: the fixture library is trusted.
    let c = unsafe { Cpp::open(&path) }.expect("load cpp library");

    // SAFETY: the fixture's functions have no preconditions beyond valid
    // pointers, which are passed.
    unsafe {
        assert_eq!(c.geo_add_int_int(2, 3), 5);
        assert_eq!(c.geo_add_double_double(1.0, 2.0), 3.5);
        let p = c.geo_translate(geo_point { x: 1, y: 2 }, &geo_point { x: 10, y: 20 });
        assert_eq!((p.x, p.y), (11, 22));
        assert_eq!(c.geo_sides(geo_shape_square), 4);
        assert_eq!(geo_mm.0 + geo_cm.0, 1);
        assert_eq!(c.geo_checked_div(6, 3), 2);
        assert_eq!(*c.geo_counter(), 3);
        assert_eq!(c.geo_Registry_count(), 7);
        assert_eq!(*c.geo_Registry_total(), 11);
        assert_eq!(c.geo_detail_version(), 2);
        assert_eq!(c.cpp_plain(21), 42);

        // Member functions take `this` first; constructors initialize it.
        let mut r = std::mem::MaybeUninit::<geo_Registry>::uninit();
        c.geo_Registry_new(r.as_mut_ptr());
        assert_eq!(c.geo_Registry_value(r.as_ptr()), 1);

        // Classes with constructors and a destructor get RAII wrappers;
        // their methods drop the `geo_Counter_` prefix, and overloads keep
        // their parameter suffix.
        assert_eq!(c.geo_Counter_live(), 0);
        {
            let mut a = GeoCounter::new_void(&c);
            let mut b = GeoCounter::new_int(&c, 5);
            assert_eq!(c.geo_Counter_live(), 2);
            a.add_int(3);
            assert_eq!(a.get(), 3);
            assert_eq!(b.add_int_int(1, 2), 8);
            assert_eq!(b.get(), 8);
        }
        assert_eq!(c.geo_Counter_live(), 0);
    }
    println!("cpp ok");
}
