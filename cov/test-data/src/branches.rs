#![feature(start)]
extern {
    fn puts(s: *const u8) -> i32;
}
#[start]
fn start(argc: isize, argv: *const *const u8) -> isize {
    unsafe {
        if argc == 1 {
            puts(b"ok!\0".as_ptr());
            if **argv == b'?' {
                puts(b"what?\0".as_ptr());
            }
        }
        0
    }
}
