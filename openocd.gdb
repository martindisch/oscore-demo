target extended-remote :3333

# print demangled symbols
set print asm-demangle on

# set backtrace limit to not have infinite backtrace loops
set backtrace limit 32

# *try* to stop at the user entry point (it might be gone due to inlining)
break main

monitor arm semihosting enable

load

# start the process but only advance to the next breakpoint (main)
continue
