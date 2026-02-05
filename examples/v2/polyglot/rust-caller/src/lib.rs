//! Rust component that calls Python calculator
//!
//! This demonstrates cross-language interoperability via the Component Model.
//! The Rust code calls functions implemented in Python, with zero FFI boilerplate.
//!
//! Build:
//!   cargo component build --release
//!
//! The resulting WASM component imports the calculator interface and can be
//! linked with any component that exports it (like our Python implementation).

// Generate bindings from WIT
wit_bindgen::generate!({
    path: "../calculator.wit",
    world: "calculator-user",
});

use crate::example::calculator::calculator;

/// Demo function that uses the calculator
pub fn demo() {
    println!("=== Rust calling Python Calculator ===\n");

    // Basic arithmetic - calling Python from Rust!
    let sum = calculator::add(10, 5);
    println!("10 + 5 = {}", sum);

    let diff = calculator::subtract(10, 5);
    println!("10 - 5 = {}", diff);

    let product = calculator::multiply(10, 5);
    println!("10 * 5 = {}", product);

    // Division with error handling
    match calculator::divide(10, 3) {
        Ok(result) => println!("10 / 3 = {}", result),
        Err(e) => println!("Division error: {}", e),
    }

    // Division by zero - Python returns an error
    match calculator::divide(10, 0) {
        Ok(result) => println!("10 / 0 = {}", result),
        Err(e) => println!("10 / 0 = Error: {}", e),
    }

    // Fibonacci
    println!("\nFibonacci sequence:");
    for n in 0..10 {
        let fib = calculator::fibonacci(n);
        print!("{} ", fib);
    }
    println!();
}

/// Main entry point when run as CLI
#[no_mangle]
pub extern "C" fn _start() {
    demo();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addition() {
        assert_eq!(calculator::add(2, 3), 5);
    }

    #[test]
    fn test_fibonacci() {
        assert_eq!(calculator::fibonacci(10), 55);
    }
}
