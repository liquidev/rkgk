use core::fmt::Write;

use alloc::string::String;

extern "C" {
    fn panic(message_len: u32, message: *const u8) -> !;
}

fn panic_impl(info: &core::panic::PanicInfo) -> ! {
    let mut message = String::new();
    _ = write!(&mut message, "{info}");

    unsafe { panic(message.len() as u32, message.as_ptr()) };
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    panic_impl(info)
}
