/// Conditionally prints to an ITM channel, only if it's available.
#[macro_export]
macro_rules! siprintln {
    ($channel:expr $(, $arg:expr)*) => {
        if $channel.is_fifo_ready() {
            iprintln!($channel $(, $arg)*);
        }
    };
}
