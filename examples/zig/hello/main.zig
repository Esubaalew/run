// Expected output:
// Hello from Zig!

const std = @import("std");

pub fn main() !void {
    std.debug.print("Hello from Zig!\n", .{});
}
