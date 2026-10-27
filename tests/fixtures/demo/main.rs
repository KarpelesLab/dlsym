//! Exercises bindings generated from `demo.h` against the compiled library.
//! Built and run by `tests/codegen.rs`.

#![allow(clippy::multiple_unsafe_ops_per_block)]

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

use std::ffi::{CStr, c_int, c_void};

use bindings::*;
use dlsym::Bindings;

unsafe extern "C" fn triple(user: *mut c_void, value: c_int) -> c_int {
    // SAFETY: `user` points to the `c_int` passed below.
    let offset = unsafe { *user.cast::<c_int>() };
    value * 3 + offset
}

fn main() {
    let path = std::env::args().nth(1).expect("library path");
    // SAFETY: the fixture library is trusted.
    let d = unsafe { Demo::open(&path) }.expect("load demo library");

    // Constants from #defines.
    assert_eq!(DEMO_VERSION, c"1.2.3");
    assert_eq!(DEMO_ESCAPED, c"tab\there \"quoted\"");
    assert_eq!(DEMO_ANSWER, 42);
    assert_eq!(DEMO_NEGATIVE, -7);
    assert_eq!(DEMO_FLAG, 1u32 << 31);
    assert_eq!(DEMO_BIG, 0x1_2345_6789);
    assert_eq!(DEMO_RATIO, 0.25f64);
    assert_eq!(DEMO_RATIO_F, 1.5f32);
    assert_eq!(DEMO_CHAR, 65);
    assert_eq!(DEMO_DERIVED, 85);

    // Enums: newtypes for named enums, plain constants for anonymous ones.
    assert_eq!(DEMO_RED, demo_color(1));
    assert_eq!(DEMO_SMALL, demo_size(-1));
    assert_eq!(DEMO_ANON_B, 11);

    // `safe` functions need no `unsafe` block.
    assert_eq!(d.demo_add(2, 3), 5);
    assert!(d.demo_is_even(4));

    // Function-like macros wrapping a call are methods too; this one
    // forwards to the `safe` `demo_add`, so it is safe as well.
    assert_eq!(d.demo_add_ten(5), 15);
    // SAFETY: plain integer arguments.
    assert_eq!(unsafe { d.demo_check_default(7) }, 1);

    // `#[optional]` functions that the library lacks return `None`.
    // SAFETY: never called, the symbol is missing.
    assert_eq!(unsafe { d.demo_not_implemented() }, None);

    // `demo_handle_get` is `safe_handle`: safe as a method of an owned
    // handle (which is always live), `unsafe` as a raw function.
    // SAFETY: `demo_handle_new` has no preconditions.
    let h = unsafe { d.demo_handle_new_owned(5) }.expect("handle");
    assert_eq!(h.get(), 5);
    drop(h);

    unsafe {
        // Globals.
        // `demo_add` and `demo_add_ten` each bumped the counter.
        assert_eq!(*d.demo_counter(), 2);
        *d.demo_counter() = 10;
        d.demo_add(0, 0);
        assert_eq!(*d.demo_counter(), 11);
        assert_eq!(*d.demo_const_global(), 1234);
        let names: *const *const std::ffi::c_char = d.demo_names();
        assert_eq!(CStr::from_ptr(*names), c"alpha");
        assert_eq!(CStr::from_ptr(*names.add(1)), c"beta");

        // Structs by value and by pointer.
        let p = d.demo_point_add(demo_point { x: 1, y: 2 }, demo_point { x: 10, y: 20 });
        assert_eq!((p.x, p.y), (11, 22));
        let r = d.demo_rect_make(1, 2, 3, 4);
        assert_eq!((r.origin.x, r.size.y), (1, 4));
        assert_eq!(d.demo_area(&r), 12);

        // Opaque handles.
        let h = d.demo_handle_new(99);
        assert_eq!(d.demo_handle_get(h), 99);
        d.demo_handle_free(h);

        // Owned handles are released exactly once, when dropped.
        let mut h = d.demo_handle_new_owned(7).expect("handle");
        // Functions taking the handle first become methods, without the
        // `demo_handle_` prefix; `const` handles borrow `self` shared.
        assert_eq!(h.get(), 7);
        h.set(11);
        assert_eq!(h.get(), 11);
        h.set(7);
        assert_eq!(d.demo_handle_get(h.as_raw()), 7);
        assert_eq!(*d.demo_freed(), 2);
        drop(h);
        assert_eq!(*d.demo_freed(), 3);
        let raw = d.demo_handle_new_owned(8).expect("handle").into_raw();
        assert_eq!(*d.demo_freed(), 3);
        d.demo_handle_free(raw);
        assert_eq!(*d.demo_freed(), 4);

        // Callbacks in both directions.
        let mut offset: c_int = 1;
        let got = d.demo_call(Some(triple), (&raw mut offset).cast(), 5);
        assert_eq!(got, (5 * 3 + 1) * 2);
        let cb = d.demo_get_callback().expect("callback");
        assert_eq!(cb(std::ptr::null_mut(), 21), 42);

        // Variadic functions.
        assert_eq!(d.demo_sum().with(|f| f(3, 1, 2, 3)), 6);

        // Array parameters decay to pointers.
        assert_eq!(d.demo_array_sum([1, 2, 3, 4].as_ptr()), 10);

        assert_eq!(d.demo_mix(DEMO_RED, DEMO_BLUE), demo_color(5));
        assert_eq!(d.demo_size_of(100), DEMO_LARGE);

        // Keyword-named function.
        assert_eq!(d.demo_type(2), 6);

        // Bitfields.
        let mut b = d.demo_bits_make();
        assert_eq!(b.a(), 5);
        assert_eq!(b.b(), -3);
        assert!(b.flag());
        assert_eq!(b.color(), DEMO_BLUE);
        assert_eq!(b.c(), 0xABCDE);
        assert_eq!(b.tail, 0x7F);
        b.set_a(6);
        b.set_b(-16);
        b.set_flag(false);
        b.set_color(DEMO_GREEN);
        b.set_c(0x12345);
        b.tail = 9;
        assert_eq!(d.demo_bits_check(b), 1);

        // Layouts match the C compiler's.
        assert_eq!(d.demo_layout(0), size_of::<demo_bits>());
        assert_eq!(d.demo_layout(1), size_of::<demo_packed>());
        assert_eq!(d.demo_layout(2), size_of::<demo_aligned>());
        assert_eq!(d.demo_layout(3), size_of::<demo_pack2>());
        assert_eq!(d.demo_layout(4), size_of::<demo_number>());
        assert_eq!(d.demo_layout(5), size_of::<demo_anon>());
        assert_eq!(d.demo_layout(6), std::mem::offset_of!(demo_anon, pair));

        // Anonymous members and named fields of anonymous types.
        let mut a = demo_anon::default();
        a.kind = 1;
        a.__anon1.as_int = 2;
        a.pair.lo = 3;
        a.pair.hi = 4;
        assert_eq!(d.demo_anon_sum(&a), 10);
    }

    // Keyword fields.
    let k = demo_keywords::default();
    let _ = (k.r#type, k.r#match, k.self_);

    // Flexible array members are zero-length arrays.
    let _: [c_int; 0] = demo_flex::default().data;

    // Typed wrappers: `_rs` methods next to the raw ones.
    assert_eq!(d.demo_strlen_rs(c"four"), 4);
    assert_eq!(d.demo_sum_bytes_rs(&[1, 2, 3]), 6);
    let mut buf = [0u8; 4];
    // SAFETY: `demo_fill` has no other requirements.
    assert_eq!(unsafe { d.demo_fill_rs(&mut buf, 7) }, 4);
    assert_eq!(buf, [7; 4]);
    assert_eq!(d.demo_parse_rs(c"42"), Ok(42));
    assert_eq!(d.demo_parse_rs(c"x"), Err(-1));
    assert_eq!(d.demo_checked_rs(5), Ok(10));
    assert_eq!(d.demo_checked_rs(-1), Err(-22));
    if cfg!(unix) {
        // Windows: the test library may use another C runtime (and errno).
        let err = d.demo_fail_errno_rs(2).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(2));
    }
    assert_eq!(d.demo_version_rs(), Some(c"demo 1.0"));
    // On owned handles, the annotations apply to the methods directly.
    // SAFETY: `demo_handle_new` has no preconditions.
    let h = unsafe { d.demo_handle_new_owned(3) }.expect("handle");
    let mut label = [0u8; 16];
    assert_eq!(h.label(&mut label), Ok(8));
    assert_eq!(&label[..9], b"handle 3\0");
    drop(h);

    // Nullable strings.
    assert_eq!(d.demo_greet_rs(Some(c"bob")), 3);
    assert_eq!(d.demo_greet_rs(None), -1);

    // An owned handle from an out-parameter is released on every path.
    // SAFETY: reading the counter only.
    let freed = || unsafe { *d.demo_freed() };
    let before = freed();
    let h = d.demo_handle_open_rs(9).expect("handle");
    assert_eq!(h.get(), 9);
    drop(h);
    assert_eq!(freed(), before + 1);
    assert_eq!(d.demo_handle_open_rs(-1).map(|_| ()), Err(-1));
    assert_eq!(freed(), before + 2, "freed on failure too");

    println!("demo ok");
}
