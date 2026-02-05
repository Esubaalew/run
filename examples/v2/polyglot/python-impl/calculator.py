"""
Python implementation of the calculator WIT interface.

This file implements the calculator interface defined in calculator.wit.
When compiled to WASM using componentize-py, it becomes a WASI component
that can be called from any other language (Rust, JavaScript, Go, etc.).

Usage:
    componentize-py -d ../calculator.wit -w calculator-world calculator -o calculator.wasm

The resulting calculator.wasm can be:
1. Called from Rust, JS, Go, or any language with WIT bindings
2. Deployed as a serverless function
3. Embedded in any WASI 0.2 runtime
"""

import calculator

class Calculator(calculator.Calculator):
    """Implementation of the calculator interface."""
    
    def add(self, a: int, b: int) -> int:
        """Add two numbers."""
        return a + b
    
    def subtract(self, a: int, b: int) -> int:
        """Subtract b from a."""
        return a - b
    
    def multiply(self, a: int, b: int) -> int:
        """Multiply two numbers."""
        return a * b
    
    def divide(self, a: int, b: int):
        """Divide a by b, returns error if b is zero."""
        if b == 0:
            return calculator.Err("Division by zero")
        return calculator.Ok(a // b)
    
    def fibonacci(self, n: int) -> int:
        """Calculate the nth Fibonacci number."""
        if n <= 1:
            return n
        
        a, b = 0, 1
        for _ in range(2, n + 1):
            a, b = b, a + b
        return b


class AdvancedMath(calculator.AdvancedMath):
    """Implementation of advanced math operations."""
    
    def factorial(self, n: int):
        """Calculate factorial of n."""
        if n < 0:
            return calculator.Err("Factorial undefined for negative numbers")
        if n > 20:
            return calculator.Err("Factorial too large (would overflow)")
        
        result = 1
        for i in range(2, n + 1):
            result *= i
        return calculator.Ok(result)
    
    def is_prime(self, n: int) -> bool:
        """Check if n is prime."""
        if n < 2:
            return False
        if n == 2:
            return True
        if n % 2 == 0:
            return False
        
        for i in range(3, int(n**0.5) + 1, 2):
            if n % i == 0:
                return False
        return True
    
    def gcd(self, a: int, b: int) -> int:
        """Calculate greatest common divisor using Euclidean algorithm."""
        while b:
            a, b = b, a % b
        return a


# Export the implementations
# The WIT bindings will pick these up automatically
