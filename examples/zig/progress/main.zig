// Expected output:
// [1] planning build
// [2] compiling kernel
// [3] deploying services

const std = @import("std");

pub fn main() !void {
    const steps = [_][]const u8{
        "planning build",
        "compiling kernel",
        "deploying services",
    };

    for (steps, 0..) |step, index| {
        std.debug.print("[{d}] {s}\n", .{ index + 1, step });
    }
}
